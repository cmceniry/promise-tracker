import { useState } from 'react';
import './App.css';
import Mermaid from './components/Mermaid';
import ContractCard from './components/ContractCard';
import 'bootstrap/dist/css/bootstrap.min.css';
import { from_yaml } from './libs/promise-tracker/contract';
import { Container, Row, Col } from 'react-bootstrap';
import { Card, Button } from 'react-bootstrap';

function App() {
  const [contracts, setContracts] = useState([]);

  const contractUpdater = (e) => {
    e.preventDefault();
    setContracts(c => c.map((contract) => {
      if (e.target.id !== contract.id) {
        return contract;
      }
      let err = null;
      try {
        if (e.target.value) {
          from_yaml(e.target.value);
        };
      } catch (e) {
        err = e.toString();
      };
      return {id: contract.id, text: e.target.value, err: err};
    }))
  };

  const addBlankContract = (e) => {
    e.preventDefault();
    setContracts([...contracts, {
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

  const deleteContract = (e) => {
    e.preventDefault();
    setContracts(c => c.filter((contract) => contract.id !== e.target.id));
  }

  return (
    <div className="App"> 
      <h1 className="header">Contract</h1>
      <Container fluid>
        <Row>
          <Col md={4}>
            <>
              {contracts.map((c) =>
                <ContractCard key={c.id} contractId={c.id} contractText={c.text} contractError={c.err} updateContract={contractUpdater} deleteContract={deleteContract}/>
              )}
            </>
            <Card><Button onClick={addBlankContract}>Add Another Contract</Button></Card>
          </Col>
          <Col md={8}>
            <Mermaid chart='sequenceDiagram\n    Alice->>John: Hello John, how are you?'></Mermaid>
          </Col>
        </Row>
      </Container>
    </div>
  );
}

export default App;
