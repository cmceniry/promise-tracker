import { useState, useEffect } from 'react';
import './App.css';
import Mermaid from './components/Mermaid';
import ContractCard from './components/ContractCard';
import 'bootstrap/dist/css/bootstrap.min.css';
import { allFromYAML, SchemaSyntaxError } from './libs/promise-tracker/contract';
import { Container, Row, Col } from 'react-bootstrap';
import { Card, Button, Form } from 'react-bootstrap';
import PromiseTracker from './libs/promise-tracker/promise-tracker';
import ptdiagram from './libs/promise-tracker/diagram';

function App() {
  const [contracts, setContracts] = useState([]);
  const [diagram, setDiagram] = useState("sequenceDiagram\nyou->>contract: enter something");
  const [dComponent, setDComponent] = useState("");
  const [dBehavior, setDBehavior] = useState("");
  const [selectedFile, setSelectedFile] = useState();

  useEffect(() => {
    const c = localStorage.getItem('contracts');
    if (c) {
      try {
        setContracts(JSON.parse(c));
      } finally {}
    }
  }, []);

  useEffect(() => {
    localStorage.setItem('contracts', JSON.stringify(contracts));
  }, [contracts]);

  useEffect(() => {
    try {
      if (contracts.length === 0) {
        setDiagram("sequenceDiagram\nyou->>contract: enter something");
        return;
      }
      if (contracts.filter((c) => c.err).length > 0) {
        return;
      }
      if (dComponent === null || dComponent === "") {
        setDiagram("sequenceDiagram\nyou->>component: enter something");
        return;
      }
      if (dBehavior === null || dBehavior === "") {
        setDiagram("sequenceDiagram\nyou->>behavior: enter something");
        return;
      }
      const pt = new PromiseTracker();
      contracts.forEach((c) => {
        if (c.text) {
          allFromYAML(c.text).forEach((comp) => pt.addComponent(comp));
        }
      });
      if (!pt.getBehaviorNames().includes(dBehavior)) {
        setDiagram("sequenceDiagram\nyou->>behavior: enter a valid behacvior");
        return;
      }
      setDiagram(ptdiagram({...pt.resolve(dBehavior), component: dComponent}));
    } catch {};
  }, [contracts, dComponent, dBehavior]);

  const updateDComponent = (e) => {
    e.preventDefault();
    setDComponent(e.target.value);
  };

  const updateDBehavior = (e) => {
    e.preventDefault();
    setDBehavior(e.target.value);
  };

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
      return {id: contract.id, text: e.target.value, err: err};
    }))
  };

  const addBlankContract = (e) => {
    e.preventDefault();
    setContracts([...contracts, {
      filename: "",
      text: "",
      err: "",
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
                  updateFilename={updateFilename}
                  updateContract={contractUpdater}
                  deleteContract={deleteContract}
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
            <Form>
              <Form.Control type="text" placeholder="Component" value={dComponent} onChange={updateDComponent} />
            {/* </Form>
            <Form> */}
              <Form.Control type="text" placeholder="Behavior" value={dBehavior} onChange={updateDBehavior} />
            </Form>
            <Mermaid chart={diagram}></Mermaid>
          </Col>
        </Row>
      </Container>
    </div>
  );
}

export default App;
