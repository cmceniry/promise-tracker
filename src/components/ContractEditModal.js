import { useState, useEffect, useCallback } from 'react';
import { Modal, Button, Form, Alert } from 'react-bootstrap';
import yaml from 'js-yaml';
import { SchemaSyntaxError } from '../libs/promise-tracker/contract';

export default function ContractEditModal({ show, contracts, onHide, onSave, schema, ajv }) {
  const [editedContracts, setEditedContracts] = useState([]);
  const [errors, setErrors] = useState({});

  // Initialize edited contracts when modal opens or contracts change
  useEffect(() => {
    if (show && contracts && contracts.length > 0) {
      setEditedContracts(contracts.map(c => ({
        id: c.id,
        filename: c.filename || '',
        text: c.text || '',
      })));
      setErrors({});
    }
  }, [show, contracts]);

  const validateContract = (contractText, contractId) => {
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
  };

  const handleFilenameChange = (contractId, value) => {
    setEditedContracts(prev => prev.map(c => 
      c.id === contractId ? { ...c, filename: value } : c
    ));
  };

  const handleTextChange = (contractId, value) => {
    setEditedContracts(prev => prev.map(c => 
      c.id === contractId ? { ...c, text: value } : c
    ));
    
    // Validate on change
    const error = validateContract(value, contractId);
    setErrors(prev => ({
      ...prev,
      [contractId]: error
    }));
  };

  const handleSave = useCallback(() => {
    // Final validation for all contracts
    const finalErrors = {};
    editedContracts.forEach(c => {
      const error = validateContract(c.text, c.id);
      if (error) {
        finalErrors[c.id] = error;
      }
    });

    if (Object.keys(finalErrors).length > 0) {
      setErrors(finalErrors);
      return;
    }

    // Save all contracts
    onSave(editedContracts);
    onHide();
  }, [editedContracts, schema, ajv, onSave, onHide]);

  // Handle Enter key to trigger save
  useEffect(() => {
    if (!show) return;

    const handleKeyDown = (e) => {
      // Trigger save on Enter, but not if user is typing in a textarea
      if (e.key === 'Enter' && e.target.tagName !== 'TEXTAREA') {
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

  if (!contracts || contracts.length === 0) {
    return null;
  }

  return (
    <Modal show={show} onHide={handleCancel} size="lg">
      <Modal.Header closeButton>
        <Modal.Title>Edit Contract{contracts.length > 1 ? 's' : ''}</Modal.Title>
      </Modal.Header>
      <Modal.Body style={{ maxHeight: '70vh', overflowY: 'auto' }}>
        {editedContracts.map((contract, index) => {
          const originalContract = contracts.find(c => c.id === contract.id);
          const contractError = errors[contract.id];
          
          return (
            <div key={contract.id} style={{ marginBottom: index < editedContracts.length - 1 ? '2rem' : '0' }}>
              <h5 style={{ marginBottom: '1rem' }}>
                {contract.filename || 'untitled-contract.yaml'}
                {contractError && (
                  <span style={{ marginLeft: '1rem', color: 'red' }}>⚠</span>
                )}
              </h5>
              <Form>
                <Form.Group className="mb-3">
                  <Form.Label>Filename</Form.Label>
                  <Form.Control
                    type="text"
                    value={contract.filename}
                    onChange={(e) => handleFilenameChange(contract.id, e.target.value)}
                    placeholder="Enter filename"
                  />
                </Form.Group>
                
                <Form.Group className="mb-3">
                  <Form.Label>Contract YAML</Form.Label>
                  <Form.Control
                    as="textarea"
                    rows={15}
                    value={contract.text}
                    onChange={(e) => handleTextChange(contract.id, e.target.value)}
                    placeholder="Enter contract YAML"
                    style={{ fontFamily: 'monospace' }}
                  />
                </Form.Group>
                
                {contractError && (
                  <Alert variant="danger">{contractError}</Alert>
                )}
              </Form>
            </div>
          );
        })}
      </Modal.Body>
      <Modal.Footer>
        <Button variant="secondary" onClick={handleCancel}>
          Cancel
        </Button>
        <Button variant="primary" onClick={handleSave}>
          Save
        </Button>
      </Modal.Footer>
    </Modal>
  );
}

