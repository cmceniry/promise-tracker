import { useState, useEffect } from 'react';
import './App.css';
import ContractCard from './components/ContractCard';
import ContractGrapher from './components/ContractGrapher';
import 'bootstrap/dist/css/bootstrap.min.css';
import { allFromYAML, SchemaSyntaxError } from './libs/promise-tracker/contract';
import { Container, Row, Col } from 'react-bootstrap';
import { Card, Button } from 'react-bootstrap';

function App() {
  const [contracts, setContracts] = useState([]);
  const [selectedFile, setSelectedFile] = useState();
  const simulations = ["A", "B", "C"];

  useEffect(() => {
    const c = localStorage.getItem('contracts');
    if (c) {
      try {
        setContracts(JSON.parse(c).map((contract) => {
          return {
            ...contract,
            sims: new Set(contract.sims)
          };
        }));
      } finally {}
    }
  }, []);

  useEffect(() => {
    if (contracts === null || contracts.length === 0) {
      return
    }
    localStorage.setItem('contracts', JSON.stringify(contracts.map((contract) => {
      return {
        ...contract,
        sims: Array.from(contract.sims)
      };
    })));
  }, [contracts]);

  const updateFilename = (e) => {
    e.preventDefault();
    setContracts(c => c.map((contract) => {
      if (e.target.id !== contract.id) {
        return contract;
      }
      return {...contract, filename: e.target.value};
    }));
  };

  const contractUpdater = (e) => {
    e.preventDefault();
    setContracts(c => c.map((contract) => {
      if (e.target.id !== contract.id) {
        return contract;
      }
      let err = null;
      try {
        if (e.target.value) {
          allFromYAML(e.target.value);
        };
      } catch (e) {
        if (e instanceof SchemaSyntaxError) {
          if (e.errors[0].message.match(/^must be/)) {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].instancePath} ${e.errors[0].message}`;
          } else {
            err = `SchemaSyntaxError: Document ${e.idx}: ${e.errors[0].message}`;
          };
        } else {
          err = e.toString();
        }
      };
      return {...contract, text: e.target.value, err: err};
    }))
  };

  const addBlankContract = (e) => {
    e.preventDefault();
    setContracts([...contracts, {
      filename: "",
      text: "",
      err: "",
      sims: new Set(simulations),
      id: (() => {
        let r = "";
        for (var i = 0; i < 16; i++) {
          r += "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".charAt(Math.floor(Math.random()*62));
        }
        return r;
      })(),
    }])
  }

  const changeFile = (e) => {
    e.preventDefault();
    setSelectedFile(e.target.files[0]);
  }
  const uploadContract = (e) => {
    e.preventDefault();
    const f = new FileReader();
    f.readAsText(selectedFile);
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
        filename: selectedFile.name,
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
      // setSelectedFile('');
    };
  }

  const deleteContract = (e) => {
    e.preventDefault();
    setContracts(c => c.filter((contract) => contract.id !== e.target.id));
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

  return (
    <div className="App"> 
      <h1 className="header">Contract</h1>
      <Container fluid>
        <Row>
          <Col md={3}>
            <>
              {contracts.map((c) =>
                <ContractCard
                  key={c.id}
                  contractId={c.id}
                  contractFilename={c.filename}
                  contractText={c.text}
                  contractError={c.err}
                  contractSims={c.sims}
                  updateFilename={updateFilename}
                  updateContract={contractUpdater}
                  deleteContract={deleteContract}
                  updateContractSim={updateContractSim}
                  simulations={simulations}
                />
              )}
            </>
            <Card><Button onClick={addBlankContract}>Add Another Contract</Button></Card>
            <Card>
              <input type="file" onChange={changeFile} />
              <Button onClick={uploadContract}>Upload</Button>
            </Card>
          </Col>
          <Col md={9}>
            <ContractGrapher contracts={contracts} simulations={simulations}/>
          </Col>
        </Row>
      </Container>
    </div>
  );
}

export default App;
