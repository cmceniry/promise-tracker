import { useState } from 'react';
import './App.css';
import Mermaid from './components/Mermaid';
import { Container, Form } from 'react-bootstrap';

function App() {
  const [mms, setMMS] = useState('sequenceDiagram\nA->> B: Query');

  const handleChange = (e) => {
    e.preventDefault();
    setMMS(e.target.value)
  }

  return (
    <div className="App"> 
      <h1 className="header">Promise Viewer</h1>
      <Container>
        <Form>
          <Form.Control
            as="textarea"
            rows="10"
            value={mms}
            onChange={handleChange}
          />
        </Form>
      </Container>
      <Mermaid chart={mms} />
    </div>
  );
}

export default App;
