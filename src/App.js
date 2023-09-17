import { useState, useEffect } from 'react';
import './App.css';
import ContractCarder from './components/ContractCarder';
import ContractGrapher from './components/ContractGrapher';
import 'bootstrap/dist/css/bootstrap.min.css';
import { Container, Row, Col } from 'react-bootstrap';
// import init, { get_schema } from './wptpkg';
import init from './wptpkg';
import { get_schema, get_pt } from './wptpkg';

function App() {
  const [schema, setSchema] = useState(null);
  const [contracts, setContracts] = useState([]);
  const simulations = ["A", "B", "C"];

  useEffect(() => {
    (async function () {
      await init();
      const schemaString = await get_schema();
      try {
        const schemaObject = JSON.parse(schemaString);
        schemaObject.$id = "/promise-tracker.json";
        schemaObject.discriminator = {propertyName: "kind"};
        setSchema(schemaObject);
      } catch (e) {
          console.log(e);
      }
      try {
        const pt = await get_pt();
        pt.add_stuff("bleh");
        console.log(pt.check("bleh"));
        console.log(pt.check("blargh"));
      } catch (e) {
        console.log(e);
      }
    })();
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

  // TODO: scroll pane for the list of ContractCards
  // TODO: highlight top level (comp/beh selection or sim) when there's a contract syntax error
  return (
    <div className="App"> 
      <Container fluid>
        <Row>
          <Col md={3} style={{overflowY: "scroll"}}>
            <ContractCarder contracts={contracts} setContracts={setContracts} simulations={simulations} schema={schema}/>
          </Col>
          <Col md={9} style={{overflowY: "scroll"}}>
            <h1 className="header">Contract</h1>
            <ContractGrapher contracts={contracts} simulations={simulations}/>
          </Col>
        </Row>
      </Container>
    </div>
  );
}

export default App;
