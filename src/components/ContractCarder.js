import React from 'react';
import { useState, useEffect } from 'react';
import { Card, Button, Modal } from 'react-bootstrap';
import { BsPlusLg, BsUpload } from 'react-icons/bs';
import ContractCard from './ContractCard'
import ContractEditModal from './ContractEditModal'
import { allFromYAML, SchemaSyntaxError } from '../libs/promise-tracker/contract'; // TODO: ptrs

import yaml from 'js-yaml';
import Ajv from 'ajv';

export default function ContractCarder({contracts, setContracts, simulations, schema}) {
  const [selectedFile, setSelectedFile] = useState();
  const [ajv, setAjv] = useState();
  const [showModal, setShowModal] = useState(false);
  const [showEditModal, setShowEditModal] = useState(false);
  const [editingContracts, setEditingContracts] = useState([]);
  const [pendingContractId, setPendingContractId] = useState(null);

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
        setEditingContracts([contract]);
        setShowEditModal(true);
      }
    }
  }, [contracts, pendingContractId]);

  const openEditModal = (contractId) => {
    const contractToEdit = contracts.find(c => c.id === contractId);
    if (contractToEdit) {
      setEditingContracts([contractToEdit]);
      setShowEditModal(true);
    }
  };

  const handleSaveEditModal = (editedContracts) => {
    setContracts(c => c.map((contract) => {
      const edited = editedContracts.find(ec => ec.id === contract.id);
      if (!edited) {
        return contract;
      }
      
      // Validate the contract
      let err = null;
      try {
        if (edited.text) {
          if (!schema) {
            throw new Error("No schema loaded");
          }
          const allDocs = yaml.loadAll(edited.text);
          const validate = ajv.getSchema("/promise-tracker.json");
          allDocs.every((d, idx) => {
            const valid = validate(d);
            if (valid) {
              return true;
            }
            err = `SchemaSyntaxError: Document ${idx}: ${validate.errors[0].instancePath} ${validate.errors[0].message}`;
            return false;
          });
        }
      } catch (e) {
        if (e instanceof SchemaSyntaxError) {
          if (e.errors[0].message.match(/^must be/)) {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].instancePath} ${e.errors[0].message}`;
          } else {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].instancePath} ${e.errors[0].message}`;
          };
        } else {
          err = e.toString();
        }
      };
      
      return {
        ...contract,
        filename: edited.filename,
        text: edited.text,
        err: err
      };
    }));
  };

  const handleCloseEditModal = () => {
    setShowEditModal(false);
    setEditingContracts([]);
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
    setPendingContractId(newId);
    setContracts([...contracts, {
      filename: "",
      text: "",
      err: "",
      sims: new Set(simulations),
      id: newId,
    }])
  }

  const uploadContract = (file) => {
    if (!file) return;
    const f = new FileReader();
    f.readAsText(file);
    f.onload = () => {
      const cText = f.result;
      let err = "";
      try {
        allFromYAML(cText);
      } catch (e) {
        if (e instanceof SchemaSyntaxError) {
          if (e.errors[0].message.match(/^must be/)) {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].instancePath} ${e.errors[0].message}`;
          } else {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].message}`;
          };
        } else {
          err = e.toString();
        };
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
      setSelectedFile(null);
      setShowModal(false);
    };
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
    setSelectedFile(null);
  }

  const handleOpenModal = (e) => {
    e.preventDefault();
    setShowModal(true);
  }

  const deleteContract = (e) => {
    e.preventDefault();
    setContracts(c => c.filter((contract) => contract.id !== e.currentTarget.id));
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

  return <div style={{ height: '100vh', overflowY: 'auto' }}>
    <>
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
        />
      )}
    </>
    <Card>
      <Button onClick={addBlankContract} aria-label="Add Another Contract"><BsPlusLg /></Button>
    </Card>
    <Card>
      <Button onClick={handleOpenModal} aria-label="Upload Contract"><BsUpload /></Button>
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
    <ContractEditModal
      show={showEditModal}
      contracts={editingContracts}
      onHide={handleCloseEditModal}
      onSave={handleSaveEditModal}
      schema={schema}
      ajv={ajv}
      simulations={simulations}
      contractSimsMap={editingContracts.reduce((acc, ec) => {
        const contract = contracts.find(c => c.id === ec.id);
        if (contract) {
          acc[ec.id] = contract.sims;
        }
        return acc;
      }, {})}
      updateContractSim={updateContractSim}
    />
  </div>
}