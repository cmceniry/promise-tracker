import { useState, useEffect, useCallback } from 'react';
import { Modal, Button, Form, Alert, Badge, Spinner } from 'react-bootstrap';
import { BsExclamationTriangle } from 'react-icons/bs';
import yaml from 'js-yaml';
import { validateFilename, generateUniqueRandomFilename } from '../utils/filenameValidation';
import { fetchServerContract, compareContracts, checkFilenameDiff } from '../utils/contractDiff';

// API utility function
const getApiBaseUrl = () => {
  return process.env.REACT_APP_API_URL || 'http://localhost:8080';
};

export default function ContractEditModal({ show, contract, onHide, onSave, onPush, schema, ajv, simulations, contractSims, updateContractSim, contracts, diffStatus: initialDiffStatus = { isDifferent: false, isLoading: false, error: null } }) {
  const [editedContract, setEditedContract] = useState(null);
  const [error, setError] = useState(null);
  const [filenameError, setFilenameError] = useState(null);
  const [pushError, setPushError] = useState(null);
  const [isPushing, setIsPushing] = useState(false);
  // Local diff status for real-time checking in modal
  const [localDiffStatus, setLocalDiffStatus] = useState({ isDifferent: false, isLoading: false, error: null });

  // Initialize edited contract when modal opens or contract changes
  useEffect(() => {
    if (show && contract) {
      setEditedContract({
        id: contract.id,
        filename: contract.filename || '',
        text: contract.text || '',
      });
      setError(null);
      setFilenameError(null);
      setPushError(null);
      setIsPushing(false);
      // Reset local diff status
      setLocalDiffStatus({ isDifferent: false, isLoading: false, error: null });
    }
  }, [show, contract]);

  // Fetch and compare server contract when modal opens
  useEffect(() => {
    if (!show || !editedContract || !editedContract.filename || editedContract.filename.trim() === '') {
      return;
    }

    const checkDiff = async () => {
      setLocalDiffStatus({ isDifferent: false, isLoading: true, error: null });
      
      try {
        // Get the contract's serverPath if available
        const contract = contracts?.find(c => c.id === editedContract.id);
        const serverPath = contract?.serverPath;
        const serverPathToCheck = serverPath || editedContract.filename;
        
        // Check for filename differences
        const filenameDiffers = checkFilenameDiff(editedContract.filename, serverPath);
        
        // Fetch server contract content
        const serverText = await fetchServerContract(serverPathToCheck);
        
        if (serverText === null) {
          // Contract not found on server - contract exists locally but not on server, so it's a diff
          // This covers: deleted/moved on server, created locally, uploaded but not on server
          setLocalDiffStatus({ isDifferent: true, isLoading: false, error: null });
        } else {
          // Compare local vs server content
          const contentDiffers = compareContracts(editedContract.text, serverText);
          // Contract is different if content differs OR filename differs
          const isDifferent = contentDiffers || filenameDiffers;
          setLocalDiffStatus({ isDifferent: isDifferent, isLoading: false, error: null });
        }
      } catch (err) {
        // Network error or server unavailable - handle gracefully
        setLocalDiffStatus({ isDifferent: false, isLoading: false, error: err.message || 'Server unavailable' });
      }
    };

    checkDiff();
  }, [show, editedContract?.filename, editedContract?.id, contracts]);

  // Update diff status when contract text or filename changes in real-time
  useEffect(() => {
    if (!show || !editedContract || !editedContract.filename || editedContract.filename.trim() === '') {
      return;
    }

    // Debounce the comparison to avoid too many requests
    const timeoutId = setTimeout(async () => {
      try {
        // Get the contract's serverPath if available
        const contract = contracts?.find(c => c.id === editedContract.id);
        const serverPath = contract?.serverPath;
        const serverPathToCheck = serverPath || editedContract.filename;
        
        // Check for filename differences
        const filenameDiffers = checkFilenameDiff(editedContract.filename, serverPath);
        
        // Fetch server contract content
        const serverText = await fetchServerContract(serverPathToCheck);
        
        if (serverText === null) {
          // Contract not found on server - contract exists locally but not on server, so it's a diff
          // This covers: deleted/moved on server, created locally, uploaded but not on server
          setLocalDiffStatus(prev => ({ ...prev, isDifferent: true }));
        } else {
          // Compare local vs server content
          const contentDiffers = compareContracts(editedContract.text, serverText);
          // Contract is different if content differs OR filename differs
          const isDifferent = contentDiffers || filenameDiffers;
          setLocalDiffStatus(prev => ({ ...prev, isDifferent: isDifferent }));
        }
      } catch (err) {
        // Silently handle errors during real-time checking
        setLocalDiffStatus(prev => ({ ...prev, error: err.message || 'Server unavailable' }));
      }
    }, 500); // 500ms debounce

    return () => clearTimeout(timeoutId);
  }, [show, editedContract?.text, editedContract?.filename, editedContract?.id, contracts]);

  const validateContract = useCallback((contractText) => {
    if (!contractText || !contractText.trim()) {
      return null;
    }

    if (!schema || !ajv) {
      return "No schema loaded";
    }

    try {
      const allDocs = yaml.loadAll(contractText);
      const validate = ajv.getSchema("/promise-tracker.json");
      
      for (let idx = 0; idx < allDocs.length; idx++) {
        const valid = validate(allDocs[idx]);
        if (!valid) {
          return `SchemaSyntaxError: Document ${idx}: ${validate.errors[0].instancePath} ${validate.errors[0].message}`;
        }
      }
      return null;
    } catch (e) {
      return e.toString();
    }
  }, [schema, ajv]);

  const handleFilenameChange = (value) => {
    setEditedContract(prev => prev ? { ...prev, filename: value } : null);
    
    // Validate filename format
    const validationError = validateFilename(value);
    if (validationError) {
      setFilenameError(validationError);
      return;
    }
    
    // Check for duplicate filename (excluding current contract)
    if (contracts && value && value.trim() !== '') {
      const duplicate = contracts.find(c => 
        c.id !== contract.id && 
        c.filename && 
        c.filename.trim() === value.trim()
      );
      if (duplicate) {
        setFilenameError(`A contract with filename "${value}" already exists.`);
        return;
      }
    }
    
    setFilenameError(null);
  };

  const handleTextChange = (value) => {
    setEditedContract(prev => prev ? { ...prev, text: value } : null);
    
    // Validate on change
    const validationError = validateContract(value);
    setError(validationError);
  };

  const handleSave = useCallback(() => {
    // Final validation
    if (!editedContract) {
      return;
    }

    // If filename is empty, generate a unique random one
    let finalFilename = editedContract.filename;
    if (!finalFilename || finalFilename.trim() === '') {
      finalFilename = generateUniqueRandomFilename(contracts || []);
      setEditedContract(prev => prev ? { ...prev, filename: finalFilename } : null);
    }

    // Validate filename format
    const filenameValidationError = validateFilename(finalFilename);
    if (filenameValidationError) {
      setFilenameError(filenameValidationError);
      return;
    }

    // Check for duplicate filename (excluding current contract)
    if (contracts && finalFilename && finalFilename.trim() !== '') {
      const duplicate = contracts.find(c => 
        c.id !== contract.id && 
        c.filename && 
        c.filename.trim() === finalFilename.trim()
      );
      if (duplicate) {
        setFilenameError(`A contract with filename "${finalFilename}" already exists.`);
        return;
      }
    }

    const validationError = validateContract(editedContract.text);
    if (validationError) {
      setError(validationError);
      return;
    }

    // Save contract with final filename
    onSave({ ...editedContract, filename: finalFilename });
    onHide();
  }, [editedContract, onSave, onHide, validateContract, contract, contracts]);

  // Handle Enter key to trigger save
  useEffect(() => {
    if (!show) return;

    const handleKeyDown = (e) => {
      // Trigger save on Enter, but not if user is typing in a textarea
      // Also trigger save on SHIFT-ENTER even when typing in textarea or input fields
      if (e.key === 'Enter' && (e.shiftKey || e.target.tagName !== 'TEXTAREA')) {
        e.preventDefault();
        handleSave();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [show, handleSave]);

  const handleCancel = () => {
    onHide();
  };

  const handlePush = useCallback(async () => {
    if (!editedContract) {
      return;
    }

    // If filename is empty, generate a unique random one
    let finalFilename = editedContract.filename;
    if (!finalFilename || finalFilename.trim() === '') {
      finalFilename = generateUniqueRandomFilename(contracts || []);
      setEditedContract(prev => prev ? { ...prev, filename: finalFilename } : null);
    }

    // Validate filename format
    const filenameValidationError = validateFilename(finalFilename);
    if (filenameValidationError) {
      setFilenameError(filenameValidationError);
      return;
    }

    // Check for duplicate filename (excluding current contract)
    if (contracts && finalFilename && finalFilename.trim() !== '') {
      const duplicate = contracts.find(c => 
        c.id !== contract.id && 
        c.filename && 
        c.filename.trim() === finalFilename.trim()
      );
      if (duplicate) {
        setFilenameError(`A contract with filename "${finalFilename}" already exists.`);
        return;
      }
    }

    // Validate contract content
    const validationError = validateContract(editedContract.text);
    if (validationError) {
      setError(validationError);
      return;
    }

    // Save contract locally first to handle unsaved edits
    const contractToSave = { ...editedContract, filename: finalFilename };
    onSave(contractToSave);

    setIsPushing(true);
    setPushError(null);

    try {
      const baseUrl = getApiBaseUrl();
      // URL encode each path segment
      const encodedPath = finalFilename.split('/')
        .filter(segment => segment.length > 0)
        .map(segment => encodeURIComponent(segment))
        .join('/');
      const url = `${baseUrl}/contracts/${encodedPath}`;

      const response = await fetch(url, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/x-yaml',
        },
        body: contractToSave.text,
      });

      if (!response.ok) {
        throw new Error(`Failed to push contract: ${response.status} ${response.statusText}`);
      }

      // Determine if filename changed
      const originalServerPath = contract?.serverPath;
      const filenameChanged = originalServerPath && finalFilename.trim() !== originalServerPath.trim();

      // Update serverPath: use new filename if changed, otherwise keep existing or set to filename
      const newServerPath = filenameChanged ? finalFilename : (originalServerPath || finalFilename);

      // Call onPush callback with updated contract data
      if (onPush) {
        onPush({
          ...contractToSave,
          serverPath: newServerPath,
        });
      }

      // Clear any errors
      setPushError(null);
    } catch (err) {
      setPushError(err.message || 'Failed to push contract to server');
    } finally {
      setIsPushing(false);
    }
  }, [editedContract, validateContract, contract, contracts, onSave, onPush]);

  if (!contract || !editedContract) {
    return null;
  }

  // Use local diff status if available, otherwise fall back to initial diff status
  const currentDiffStatus = localDiffStatus.isLoading || localDiffStatus.error !== null 
    ? localDiffStatus 
    : (localDiffStatus.isDifferent ? localDiffStatus : initialDiffStatus);

  return (
    <Modal 
      show={show} 
      onHide={handleCancel} 
      size="lg"
    >
      <Modal.Header 
        closeButton
        style={{
          borderTop: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderLeft: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderRight: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderBottom: currentDiffStatus.isDifferent ? '2px solid #ffc107' : undefined,
          backgroundColor: currentDiffStatus.isDifferent ? 'rgba(255, 193, 7, 0.1)' : undefined,
        }}
      >
        <Modal.Title style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          Edit Contract
          {currentDiffStatus.isLoading && (
            <Spinner animation="border" size="sm" style={{ color: '#6c757d' }} />
          )}
          {!currentDiffStatus.isLoading && currentDiffStatus.isDifferent && (
            <Badge bg="warning" style={{ display: 'flex', alignItems: 'center', gap: '0.25rem' }}>
              <BsExclamationTriangle size={14} />
              <span>Diff</span>
            </Badge>
          )}
        </Modal.Title>
      </Modal.Header>
      <Modal.Body 
        style={{ 
          maxHeight: '70vh', 
          overflowY: 'auto',
          borderLeft: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderRight: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          backgroundColor: currentDiffStatus.isDifferent ? 'rgba(255, 193, 7, 0.05)' : undefined,
        }}
      >
        <div>
          <h5 style={{ marginBottom: '1rem', display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            {editedContract.filename || 'untitled-contract.yaml'}
            {error && (
              <span style={{ color: 'red' }}>⚠</span>
            )}
          </h5>
          {currentDiffStatus.isDifferent && (
            <Alert variant="warning" style={{ marginBottom: '1rem' }}>
              <BsExclamationTriangle style={{ marginRight: '0.5rem' }} />
              This contract differs from the server version.
            </Alert>
          )}
          {pushError && (
            <Alert variant="danger" style={{ marginBottom: '1rem' }}>
              {pushError}
            </Alert>
          )}
          {simulations && contractSims && updateContractSim && (
            <div style={{ marginBottom: '1rem', display: 'flex', gap: '0.25rem' }}>
              {simulations.map((s, i) => {
                return <Button key={i} id={editedContract.id + ":" + s} variant={contractSims.has(s) ? "success" : "danger"} onClick={updateContractSim} size="sm">{s}</Button>
              })}
            </div>
          )}
          <Form>
            <Form.Group className="mb-3">
              <Form.Label>Filename</Form.Label>
              <Form.Control
                type="text"
                value={editedContract.filename}
                onChange={(e) => handleFilenameChange(e.target.value)}
                placeholder="Enter filename"
                isInvalid={!!filenameError}
              />
              {filenameError && (
                <Form.Text className="text-danger">{filenameError}</Form.Text>
              )}
            </Form.Group>
            
            <Form.Group className="mb-3">
              <Form.Label>Contract YAML</Form.Label>
              <Form.Control
                as="textarea"
                rows={15}
                value={editedContract.text}
                onChange={(e) => handleTextChange(e.target.value)}
                placeholder="Enter contract YAML"
                style={{ fontFamily: 'monospace' }}
              />
            </Form.Group>
            
            {error && (
              <Alert variant="danger">{error}</Alert>
            )}
          </Form>
        </div>
      </Modal.Body>
      <Modal.Footer
        style={{
          borderBottom: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderLeft: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          borderRight: currentDiffStatus.isDifferent ? '3px solid #ffc107' : undefined,
          backgroundColor: currentDiffStatus.isDifferent ? 'rgba(255, 193, 7, 0.05)' : undefined,
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <Button 
          variant="primary" 
          onClick={handlePush}
          disabled={isPushing || !!error || !!filenameError || !currentDiffStatus.isDifferent}
        >
          {isPushing ? (
            <>
              <Spinner animation="border" size="sm" style={{ marginRight: '0.5rem' }} />
              Pushing...
            </>
          ) : (
            'Push'
          )}
        </Button>
        <div style={{ display: 'flex', gap: '0.5rem' }}>
          <Button variant="secondary" onClick={handleCancel}>
            Cancel
          </Button>
          <Button variant="primary" onClick={handleSave}>
            Close
          </Button>
        </div>
      </Modal.Footer>
    </Modal>
  );
}

