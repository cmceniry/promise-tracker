import { useState } from 'react';
import './App.css';
import Mermaid from './components/Mermaid';
import ContractInput from './components/ContractInput';
import 'bootstrap/dist/css/bootstrap.min.css';

function App() {

  const [contractText, setContractText] = useState("");
  const updateContract = (e) => {
    e.preventDefault();
    setContractText(e.target.value);
  };

  return (
    <div className="App"> 
      <h1 className="header">Contract</h1>
      <ContractInput contractText={contractText} updateContract={updateContract}/>
    </div>
  );
}

export default App;
