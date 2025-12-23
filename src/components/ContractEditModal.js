import { useState, useEffect, useCallback } from 'react';
import { Modal, Button, Form, Alert } from 'react-bootstrap';
import yaml from 'js-yaml';
import { SchemaSyntaxError } from '../libs/promise-tracker/contract';
import { validateFilename, generateUniqueRandomFilename } from '../utils/filenameValidation';

export default function ContractEditModal({ show, contract, onHide, onSave, schema, ajv, simulations, contractSims, updateContractSim, contracts }) {
  const [editedContract, setEditedContract] = useState(null);
  const [error, setError] = useState(null);
  const [filenameError, setFilenameError] = useState(null);

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
    }
  }, [show, contract]);

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
      if (e instanceof SchemaSyntaxError) {
        return `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].instancePath} ${e.errors[0].message}`;
      } else {
        return e.toString();
      }
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

  if (!contract || !editedContract) {
    return null;
  }

  return (
    <Modal show={show} onHide={handleCancel} size="lg">
      <Modal.Header closeButton>
        <Modal.Title>Edit Contract</Modal.Title>
      </Modal.Header>
      <Modal.Body style={{ maxHeight: '70vh', overflowY: 'auto' }}>
        <div>
          <h5 style={{ marginBottom: '1rem' }}>
            {editedContract.filename || 'untitled-contract.yaml'}
            {error && (
              <span style={{ marginLeft: '1rem', color: 'red' }}>⚠</span>
            )}
          </h5>
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
      <Modal.Footer>
        <Button variant="secondary" onClick={handleCancel}>
          Cancel
        </Button>
        <Button variant="primary" onClick={handleSave}>
          Close
        </Button>
      </Modal.Footer>
    </Modal>
  );
}

