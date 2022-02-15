import { useState } from 'react';
import './App.css';
import Mermaid from './components/Mermaid';
import ContractCard from './components/ContractCard';
import 'bootstrap/dist/css/bootstrap.min.css';
import { from_yaml } from './libs/promise-tracker/contract';

function App() {
  const [contracts, setContracts] = useState([
    {
      text: "",
      err: null,
      id: "con-0",
    },
    {
      text: "",
      err: null,
      id: "con-1",
    },
  ]);

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

  return (
    <div className="App"> 
      <h1 className="header">Contract</h1>
      <>
      {contracts.map((c) =>
        <ContractCard key={c.id} contractId={c.id} contractText={c.text} contractError={c.err} updateContract={contractUpdater}/>
      )}
      </>
    </div>
  );
}

export default App;
