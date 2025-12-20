import { useEffect, useRef, useState, useMemo } from 'react';
import { Alert, Button, Card, Form, Badge } from 'react-bootstrap';
import yaml from 'js-yaml';
import './ContractCard.css';

// Utility function to extract agents and superagents from contract text
function extractAgentsAndSuperagents(contractText) {
  const agents = [];
  const superagents = [];
  
  if (!contractText || contractText.trim() === '') {
    return { agents, superagents };
  }
  
  try {
    const documents = yaml.loadAll(contractText);
    documents.forEach((doc) => {
      if (!doc || typeof doc !== 'object') {
        return;
      }
      
      const kind = doc.kind;
      const name = doc.name;
      
      if (name && typeof name === 'string') {
        // Handle both Agent/Component and SuperAgent/Collective formats
        if (kind === 'Agent' || kind === 'Component' || (!kind)) {
          // Default to Agent/Component if kind is not specified
          agents.push(name);
        } else if (kind === 'SuperAgent' || kind === 'Collective') {
          superagents.push(name);
        }
      }
    });
  } catch (e) {
    // If parsing fails, return empty arrays
    return { agents, superagents };
  }
  
  return { agents, superagents };
}

export default function ContractCard({contractId, contractFilename, contractText, contractError, contractSims, updateFilename, updateContract, deleteContract, updateContractSim, simulations, cardClassName}) {
  const downloadRef = useRef("");
  const [downloadLink, setDownloadLink] = useState("");
  const [isExpanded, setIsExpanded] = useState(false);
  
  useEffect(() => {
    const d = new Blob([contractText], { type: 'text/json' });
    if (downloadRef.current !== "") window.URL.revokeObjectURL(downloadRef.current);
    downloadRef.current = window.URL.createObjectURL(d);
    setDownloadLink(downloadRef.current);
  }, [contractText]);

  const { agents, superagents } = useMemo(() => extractAgentsAndSuperagents(contractText), [contractText]);

  const toggleExpand = () => {
    setIsExpanded(!isExpanded);
  };

  return <Card body className={cardClassName}>
    <Form>
      <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.5rem' }}>
        <Form.Control
          id={contractId}
          as="input"
          value={contractFilename}
          onChange={updateFilename}
          style={{ flex: 1 }}
        />
        <Button 
          variant="outline-secondary" 
          onClick={toggleExpand}
          style={{ minWidth: '2rem', padding: '0.25rem 0.5rem' }}
          aria-label={isExpanded ? "Collapse" : "Expand"}
        >
          {isExpanded ? '▼' : '▶'}
        </Button>
      </div>
      
      {/* Agents and Superagents - always visible */}
      {(agents.length > 0 || superagents.length > 0) && (
        <div style={{ marginBottom: '0.5rem', fontSize: '0.9em' }}>
          {agents.length > 0 && (
            <div style={{ marginBottom: '0.25rem' }}>
              <strong>Agents:</strong>{' '}
              {agents.map((agent, i) => (
                <Badge key={i} bg="primary" style={{ marginRight: '0.25rem' }}>{agent}</Badge>
              ))}
            </div>
          )}
          {superagents.length > 0 && (
            <div>
              <strong>Superagents:</strong>{' '}
              {superagents.map((superagent, i) => (
                <Badge key={i} bg="info" style={{ marginRight: '0.25rem' }}>{superagent}</Badge>
              ))}
            </div>
          )}
        </div>
      )}
      
      {/* Simulation buttons - always visible */}
      <div style={{ marginBottom: '0.5rem' }}>
        {simulations.map((s, i) => {
          return <Button key={i} id={contractId + ":" + s} variant={contractSims.has(s) ? "success" : "danger"} onClick={updateContractSim} size="sm" style={{ marginRight: '0.25rem' }}>{s}</Button>
        })}
      </div>
      
      {/* Error alert - always visible */}
      {contractError && <Alert variant="danger" style={{ marginBottom: '0.5rem' }}>{contractError}</Alert>}
      
      {/* Expanded content */}
      {isExpanded && (
        <div className="contract-card-expanded">
          <Form.Control
            id={contractId}
            as="textarea"
            rows="10"
            value={contractText}
            onChange={updateContract}
            style={{ marginBottom: '0.5rem' }}
          />
          <div>
            <a download={contractFilename} href={downloadLink}><Button size="sm">Download</Button></a>{' '}
            <Button id={contractId} onClick={deleteContract} size="sm" variant="danger">Delete</Button>
          </div>
        </div>
      )}
    </Form>
  </Card>
}
