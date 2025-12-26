import { useState, useEffect, useCallback, useMemo } from 'react';
import { Modal, Button, Form, Alert, Badge, Spinner, Collapse } from 'react-bootstrap';
import { BsExclamationTriangle, BsChevronDown, BsChevronRight } from 'react-icons/bs';
import yaml from 'js-yaml';
import { diffLines } from 'diff';
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
  // Store server text for diff computation
  const [serverText, setServerText] = useState(null);
  // Collapsible diff display state
  const [showDiff, setShowDiff] = useState(false);

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
      // Reset server text and diff display
      setServerText(null);
      setShowDiff(false);
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
        const fetchedServerText = await fetchServerContract(serverPathToCheck);
        
        // Store server text for diff computation
        setServerText(fetchedServerText);
        
        if (fetchedServerText === null) {
          // Contract not found on server - contract exists locally but not on server, so it's a diff
          // This covers: deleted/moved on server, created locally, uploaded but not on server
          setLocalDiffStatus({ isDifferent: true, isLoading: false, error: null });
        } else {
          // Compare local vs server content
          const contentDiffers = compareContracts(editedContract.text, fetchedServerText);
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
  }, [show, editedContract, contracts]);

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
        const fetchedServerText = await fetchServerContract(serverPathToCheck);
        
        // Store server text for diff computation
        setServerText(fetchedServerText);
        
        if (fetchedServerText === null) {
          // Contract not found on server - contract exists locally but not on server, so it's a diff
          // This covers: deleted/moved on server, created locally, uploaded but not on server
          setLocalDiffStatus(prev => ({ ...prev, isDifferent: true }));
        } else {
          // Compare local vs server content
          const contentDiffers = compareContracts(editedContract.text, fetchedServerText);
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
  }, [show, editedContract, contracts]);

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

  // Use local diff status if available, otherwise fall back to initial diff status
  const currentDiffStatus = useMemo(() => {
    return localDiffStatus.isLoading || localDiffStatus.error !== null 
      ? localDiffStatus 
      : (localDiffStatus.isDifferent ? localDiffStatus : initialDiffStatus);
  }, [localDiffStatus, initialDiffStatus]);

  // Compute diff lines with proper text preprocessing
  const computeDiffLines = useCallback((newText, oldText) => {
    // Handle null oldText (contract doesn't exist on server)
    const normalizedOld = oldText === null ? '' : oldText.trim() + '\n';
    const normalizedNew = (newText || '').trim() + '\n';
    
    // Use diff library to generate line-by-line diff
    const diff = diffLines(normalizedOld, normalizedNew);
    return diff;
  }, []);

  // Collapse diff when there's no difference
  useEffect(() => {
    if (!currentDiffStatus.isDifferent) {
      setShowDiff(false);
    }
  }, [currentDiffStatus.isDifferent]);

  // Compute diff when we have both texts and there's a difference
  const diffResult = useMemo(() => {
    if (!currentDiffStatus.isDifferent || !editedContract || currentDiffStatus.isLoading) {
      return null;
    }
    return computeDiffLines(editedContract.text, serverText);
  }, [currentDiffStatus.isDifferent, currentDiffStatus.isLoading, editedContract, serverText, computeDiffLines]);

  // Render side-by-side diff view
  const renderDiffView = () => {
    if (!diffResult) {
      return null;
    }

    // Build aligned lines for side-by-side display
    const leftLines = [];
    const rightLines = [];

    // Process diff parts to align consecutive removed+added pairs
    for (let i = 0; i < diffResult.length; i++) {
      const part = diffResult[i];
      const lines = part.value.split('\n');
      // Remove the trailing empty line from split
      if (lines.length > 0 && lines[lines.length - 1] === '') {
        lines.pop();
      }

      if (part.added) {
        // Added lines: show in left column, empty in right
        lines.forEach((line) => {
          leftLines.push({ text: line, type: 'added' });
          rightLines.push({ text: '', type: 'empty' });
        });
      } else if (part.removed) {
        // Check if next part is added (for alignment)
        const nextPart = i + 1 < diffResult.length ? diffResult[i + 1] : null;
        if (nextPart && nextPart.added) {
          // Align removed with added - show them side-by-side
          const nextLines = nextPart.value.split('\n');
          if (nextLines.length > 0 && nextLines[nextLines.length - 1] === '') {
            nextLines.pop();
          }
          const maxLines = Math.max(lines.length, nextLines.length);
          for (let j = 0; j < maxLines; j++) {
            if (j < lines.length && j < nextLines.length) {
              // Both sides have content - show change
              leftLines.push({ text: nextLines[j], type: 'added' });
              rightLines.push({ text: lines[j], type: 'removed' });
            } else if (j < lines.length) {
              // Only removed side has content
              leftLines.push({ text: '', type: 'empty' });
              rightLines.push({ text: lines[j], type: 'removed' });
            } else {
              // Only added side has content
              leftLines.push({ text: nextLines[j], type: 'added' });
              rightLines.push({ text: '', type: 'empty' });
            }
          }
          // Skip the next part since we've already processed it
          i++;
        } else {
          // Removed lines without matching added: show in right column, empty in left
          lines.forEach((line) => {
            leftLines.push({ text: '', type: 'empty' });
            rightLines.push({ text: line, type: 'removed' });
          });
        }
      } else {
        // Unchanged lines: show in both columns
        lines.forEach((line) => {
          leftLines.push({ text: line, type: 'unchanged' });
          rightLines.push({ text: line, type: 'unchanged' });
        });
      }
    }

    return (
      <div style={{ 
        display: 'grid', 
        gridTemplateColumns: '1fr 1fr', 
        gap: '1px',
        border: '1px solid #dee2e6',
        borderRadius: '4px',
        overflow: 'hidden',
        fontFamily: 'monospace',
        fontSize: '0.9em',
        maxHeight: '400px',
        overflowY: 'auto'
      }}>
        {/* Left column - New code */}
        <div style={{ backgroundColor: '#fff' }}>
          <div style={{ 
            padding: '0.5rem', 
            backgroundColor: '#f6f8fa', 
            borderBottom: '1px solid #dee2e6',
            fontWeight: 'bold',
            fontSize: '0.85em'
          }}>
            New (Local)
          </div>
          {leftLines.map((line, idx) => (
            <div
              key={`left-${idx}`}
              style={{
                padding: '0.25rem 0.5rem',
                backgroundColor: line.type === 'added' 
                  ? '#d4edda' 
                  : line.type === 'unchanged' 
                  ? '#f6f8fa' 
                  : '#fff',
                whiteSpace: 'pre-wrap',
                minHeight: '1.5em'
              }}
            >
              {line.text}
            </div>
          ))}
        </div>
        
        {/* Right column - Old code */}
        <div style={{ backgroundColor: '#fff' }}>
          <div style={{ 
            padding: '0.5rem', 
            backgroundColor: '#f6f8fa', 
            borderBottom: '1px solid #dee2e6',
            fontWeight: 'bold',
            fontSize: '0.85em'
          }}>
            Old (Server)
          </div>
          {rightLines.map((line, idx) => (
            <div
              key={`right-${idx}`}
              style={{
                padding: '0.25rem 0.5rem',
                backgroundColor: line.type === 'removed' 
                  ? '#f8d7da' 
                  : line.type === 'unchanged' 
                  ? '#f6f8fa' 
                  : '#fff',
                whiteSpace: 'pre-wrap',
                minHeight: '1.5em'
              }}
            >
              {line.text}
            </div>
          ))}
        </div>
      </div>
    );
  };

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
            <div style={{ marginBottom: '1rem' }}>
              <Button
                variant="outline-warning"
                onClick={() => setShowDiff(!showDiff)}
                style={{
                  width: '100%',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '0.5rem 1rem'
                }}
              >
                <span style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  {showDiff ? <BsChevronDown /> : <BsChevronRight />}
                  <BsExclamationTriangle />
                  <span>Show diff with server version</span>
                </span>
              </Button>
              <Collapse in={showDiff}>
                <div style={{ marginTop: '1rem' }}>
                  {renderDiffView()}
                </div>
              </Collapse>
            </div>
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

