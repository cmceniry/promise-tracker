import React from 'react';
import { useState, useEffect } from 'react';
import { Card, Button, Modal } from 'react-bootstrap';
import { BsPlusLg, BsUpload, BsCloudDownload } from 'react-icons/bs';
import ContractCard from './ContractCard'
import ContractEditModal from './ContractEditModal'
import ContractBrowser from './ContractBrowser'
import { DndContext, closestCenter } from '@dnd-kit/core';
import { SortableContext, verticalListSortingStrategy } from '@dnd-kit/sortable';
import { validateFilename, generateUniqueRandomFilename } from '../utils/filenameValidation';
import { fetchServerContract, compareContracts, checkFilenameDiff } from '../utils/contractDiff';

import yaml from 'js-yaml';
import Ajv from 'ajv';


export default function ContractCarder({contracts, setContracts, simulations, schema}) {
  const [ajv, setAjv] = useState();
  const [showModal, setShowModal] = useState(false);
  const [showBrowserModal, setShowBrowserModal] = useState(false);
  const [showEditModal, setShowEditModal] = useState(false);
  const [editingContract, setEditingContract] = useState(null);
  const [pendingContractId, setPendingContractId] = useState(null);
  // Map<contractId, { isDifferent: boolean, isLoading: boolean, error: string | null }>
  const [diffStatus, setDiffStatus] = useState(new Map());

  useEffect(() => {
    if (!schema) {
      return;
    }
    setAjv(new Ajv({
      schemas: [schema],
      discriminator: true,
    }));
  }, [schema]);

  // Auto-open modal when a new contract is created
  useEffect(() => {
    if (pendingContractId) {
      const contract = contracts.find(c => c.id === pendingContractId);
      if (contract) {
        setPendingContractId(null);
        setEditingContract(contract);
        setShowEditModal(true);
      }
    }
  }, [contracts, pendingContractId]);

  // Check diff status for contracts when contracts change
  useEffect(() => {
    if (contracts.length === 0) {
      setDiffStatus(new Map());
      return;
    }

    // Check each contract that has a filename against the server
    const checkContractDiffs = async () => {
      const newDiffStatus = new Map();
      
      // Initialize all contracts as loading
      contracts.forEach(contract => {
        if (contract.filename && contract.filename.trim() !== '') {
          newDiffStatus.set(contract.id, {
            isDifferent: false,
            isLoading: true,
            error: null
          });
        }
      });

      setDiffStatus(newDiffStatus);

      // Check each contract
      for (const contract of contracts) {
        // Skip contracts without filenames (blank contracts)
        if (!contract.filename || contract.filename.trim() === '') {
          continue;
        }

        try {
          // Determine server path to check: use serverPath if available (original association), otherwise use current filename
          const serverPathToCheck = contract.serverPath || contract.filename;
          
          // Check for filename differences first
          const filenameDiffers = checkFilenameDiff(contract.filename, contract.serverPath);
          
          // Fetch server contract content
          const serverText = await fetchServerContract(serverPathToCheck);
          
          if (serverText === null) {
            // Contract not found on server - contract exists locally but not on server, so it's a diff
            // This covers: deleted/moved on server, created locally, uploaded but not on server
            setDiffStatus(prev => {
              const next = new Map(prev);
              next.set(contract.id, {
                isDifferent: true, // Always show diff when contract exists locally but not on server
                isLoading: false,
                error: null
              });
              return next;
            });
          } else {
            // Compare local vs server content
            const contentDiffers = compareContracts(contract.text, serverText);
            // Contract is different if content differs OR filename differs
            const isDifferent = contentDiffers || filenameDiffers;
            setDiffStatus(prev => {
              const next = new Map(prev);
              next.set(contract.id, {
                isDifferent: isDifferent,
                isLoading: false,
                error: null
              });
              return next;
            });
          }
        } catch (err) {
          // Network error or server unavailable - handle gracefully
          setDiffStatus(prev => {
            const next = new Map(prev);
            next.set(contract.id, {
              isDifferent: false,
              isLoading: false,
              error: err.message || 'Server unavailable'
            });
            return next;
          });
        }
      }
    };

    checkContractDiffs();
  }, [contracts]);

  const openEditModal = (contractId) => {
    const contractToEdit = contracts.find(c => c.id === contractId);
    if (contractToEdit) {
      setEditingContract(contractToEdit);
      setShowEditModal(true);
    }
  };

  const handleSaveEditModal = (editedContract) => {
    setContracts(c => c.map((contract) => {
      if (contract.id !== editedContract.id) {
        return contract;
      }
      
      // Validate the contract
      let err = null;
      try {
        if (editedContract.text) {
          if (!schema || !ajv) {
            err = "No schema loaded";
          } else {
            const allDocs = yaml.loadAll(editedContract.text);
            const validate = ajv.getSchema("/promise-tracker.json");
            for (let idx = 0; idx < allDocs.length; idx++) {
              const valid = validate(allDocs[idx]);
              if (!valid) {
                err = `SchemaSyntaxError: Document ${idx}: ${validate.errors[0].instancePath} ${validate.errors[0].message}`;
                break;
              }
            }
          }
        }
      } catch (e) {
        err = e.toString();
      };
      
      return {
        ...contract,
        filename: editedContract.filename,
        text: editedContract.text,
        err: err
      };
    }));
  };

  const handlePush = (pushedContract) => {
    setContracts(c => c.map((contract) => {
      if (contract.id !== pushedContract.id) {
        return contract;
      }
      
      // Update contract with new serverPath (and filename if changed)
      return {
        ...contract,
        filename: pushedContract.filename,
        text: pushedContract.text,
        serverPath: pushedContract.serverPath,
      };
    }));
  };

  const handleCloseEditModal = () => {
    setShowEditModal(false);
    setEditingContract(null);
  };

  const addBlankContract = (e) => {
    e.preventDefault();
    const newId = (() => {
      let r = "";
      for (var i = 0; i < 16; i++) {
        r += "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".charAt(Math.floor(Math.random()*62));
      }
      return r;
    })();
    // Generate a unique random filename for the new contract
    const randomFilename = generateUniqueRandomFilename(contracts);
    setPendingContractId(newId);
    setContracts([...contracts, {
      filename: randomFilename,
      text: "",
      err: "",
      sims: new Set(simulations),
      id: newId,
    }])
  }

  const uploadContract = (file) => {
    if (!file) return;
    
    // Validate filename format
    const filenameValidationError = validateFilename(file.name);
    if (filenameValidationError) {
      alert(`Invalid filename: ${filenameValidationError}\n\nFile: ${file.name}`);
      return;
    }

    // Check for duplicate filename
    const existingFilenames = contracts
      .map(c => c.filename)
      .filter(f => f && f.trim() !== '');
    if (existingFilenames.includes(file.name)) {
      alert(`A contract with filename "${file.name}" already exists. Please rename the file or remove the existing contract.`);
      return;
    }

    const f = new FileReader();
    f.readAsText(file);
    f.onload = () => {
      const cText = f.result;
      let err = "";
      try {
        if (cText && cText.trim()) {
          if (!schema || !ajv) {
            err = "No schema loaded";
          } else {
            const allDocs = yaml.loadAll(cText);
            const validate = ajv.getSchema("/promise-tracker.json");
            for (let idx = 0; idx < allDocs.length; idx++) {
              const valid = validate(allDocs[idx]);
              if (!valid) {
                err = `SchemaSyntaxError: Document ${idx}: ${validate.errors[0].instancePath} ${validate.errors[0].message}`;
                break;
              }
            }
          }
        }
      } catch (e) {
        err = e.toString();
      };
      setContracts([...contracts, {
        filename: file.name,
        text: cText,
        err: err,
        sims: new Set(simulations),
        id: (() => {
          let r = "";
          for (var i = 0; i < 16; i++) {
            r += "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".charAt(Math.floor(Math.random()*62));
          }
          return r;
        })(),
      }]);
      setShowModal(false);
    };
  }

  const loadContractFromAPI = (contractId, contractFilename, contractContent) => {
    let err = "";
    if (contractContent && contractContent.trim()) {
      try {
        if (!schema || !ajv) {
          err = "No schema loaded";
        } else {
          const allDocs = yaml.loadAll(contractContent);
          const validate = ajv.getSchema("/promise-tracker.json");
          for (let idx = 0; idx < allDocs.length; idx++) {
            const valid = validate(allDocs[idx]);
            if (!valid) {
              err = `SchemaSyntaxError: Document ${idx}: ${validate.errors[0].instancePath} ${validate.errors[0].message}`;
              break;
            }
          }
        }
      } catch (e) {
        err = e.toString();
      };
    }
    setContracts([...contracts, {
      filename: contractId, // Use the full server contract path (contractId) as the filename to maintain association
      serverPath: contractId, // Store the original server path to detect filename changes
      text: contractContent,
      err: err,
      sims: new Set(simulations),
      id: (() => {
        let r = "";
        for (var i = 0; i < 16; i++) {
          r += "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".charAt(Math.floor(Math.random()*62));
        }
        return r;
      })(),
    }]);
  }

  const handleOpenBrowserModal = (e) => {
    e.preventDefault();
    setShowBrowserModal(true);
  }

  const changeFile = (e) => {
    e.preventDefault();
    const file = e.target.files[0];
    if (file) {
      uploadContract(file);
      // Reset the input so the same file can be selected again
      e.target.value = '';
    }
  }

  const handleCloseModal = () => {
    setShowModal(false);
  }

  const handleOpenModal = (e) => {
    e.preventDefault();
    setShowModal(true);
  }

  const deleteContract = (contractId) => {
    setContracts(c => c.filter((contract) => contract.id !== contractId));
  }

  const updateContractSim = (e) => {
    e.preventDefault();
    const eId = e.target.id.split(":")
    if (eId.length !== 2) {
      return;
    }
    setContracts(c => c.map((contract) => {
      if (eId[0] !== contract.id) {
        return contract;
      };
      const s = new Set(contract.sims);
      if (s.has(eId[1])) {
        s.delete(eId[1]);
      } else {
        s.add(eId[1]);
      };
      return {...contract, sims: s};
    }));
  }

  const handleDragEnd = (event) => {
    const { active, over } = event;
    
    if (over && active.id !== over.id) {
      setContracts((items) => {
        const oldIndex = items.findIndex((item) => item.id === active.id);
        const newIndex = items.findIndex((item) => item.id === over.id);
        
        const newItems = [...items];
        const [removed] = newItems.splice(oldIndex, 1);
        newItems.splice(newIndex, 0, removed);
        
        return newItems;
      });
    }
  }

  return <div style={{ height: '100vh', overflowY: 'auto' }}>
    <DndContext collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <SortableContext items={contracts.map(c => c.id)} strategy={verticalListSortingStrategy}>
        {contracts.map((c, i) =>
          <ContractCard
            key={c.id}
            contractId={c.id}
            contractFilename={c.filename}
            contractText={c.text}
            contractError={c.err}
            contractSims={c.sims}
            deleteContract={deleteContract}
            updateContractSim={updateContractSim}
            simulations={simulations}
            cardClassName={i % 2 === 0 ? 'contract-card-even' : 'contract-card-odd'}
            onEdit={openEditModal}
            diffStatus={diffStatus.get(c.id) || { isDifferent: false, isLoading: false, error: null }}
          />
        )}
      </SortableContext>
    </DndContext>
    <Card>
      <Button onClick={addBlankContract} aria-label="Add Another Contract"><BsPlusLg /></Button>
    </Card>
    <Card>
      <Button onClick={handleOpenModal} aria-label="Upload Contract"><BsUpload /></Button>
    </Card>
    <Card>
      <Button onClick={handleOpenBrowserModal} aria-label="Load Contract from API"><BsCloudDownload /></Button>
    </Card>
    <Modal show={showModal} onHide={handleCloseModal}>
      <Modal.Header closeButton>
        <Modal.Title>Upload Contract File</Modal.Title>
      </Modal.Header>
      <Modal.Body>
        <input type="file" onChange={changeFile} accept=".yaml,.yml" />
      </Modal.Body>
      <Modal.Footer>
        <Button variant="secondary" onClick={handleCloseModal}>
          Cancel
        </Button>
      </Modal.Footer>
    </Modal>
    <ContractBrowser
      show={showBrowserModal}
      onHide={() => setShowBrowserModal(false)}
      onSelectContract={loadContractFromAPI}
    />
    <ContractEditModal
      show={showEditModal}
      contract={editingContract}
      onHide={handleCloseEditModal}
      onSave={handleSaveEditModal}
      onPush={handlePush}
      schema={schema}
      ajv={ajv}
      simulations={simulations}
      contractSims={editingContract ? (contracts.find(c => c.id === editingContract.id)?.sims || new Set()) : new Set()}
      updateContractSim={updateContractSim}
      contracts={contracts}
      diffStatus={editingContract ? (diffStatus.get(editingContract.id) || { isDifferent: false, isLoading: false, error: null }) : { isDifferent: false, isLoading: false, error: null }}
    />
  </div>
}