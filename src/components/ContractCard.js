import { useEffect, useRef, useState, useMemo } from 'react';
import { Alert, Button, Card, Badge } from 'react-bootstrap';
import { BsDownload, BsTrash, BsGripVertical } from 'react-icons/bs';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
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

export default function ContractCard({contractId, contractFilename, contractText, contractError, contractSims, deleteContract, updateContractSim, simulations, cardClassName, onEdit}) {
  const downloadRef = useRef("");
  const [downloadLink, setDownloadLink] = useState("");
  
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: contractId });
  
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };
  
  useEffect(() => {
    const d = new Blob([contractText], { type: 'text/json' });
    if (downloadRef.current !== "") window.URL.revokeObjectURL(downloadRef.current);
    downloadRef.current = window.URL.createObjectURL(d);
    setDownloadLink(downloadRef.current);
  }, [contractText]);

  const { agents, superagents } = useMemo(() => extractAgentsAndSuperagents(contractText), [contractText]);

  const handleCardClick = (e) => {
    // Don't trigger edit if clicking on action buttons, links, or drag handle
    if (e.target.closest('button') || e.target.closest('a') || e.target.closest('.drag-handle')) {
      return;
    }
    if (onEdit) {
      onEdit(contractId);
    }
  };

  return <Card 
    ref={setNodeRef}
    body 
    className={cardClassName}
    onClick={handleCardClick}
    style={{ ...style, cursor: 'pointer', position: 'relative' }}
  >
      <div 
        className="drag-handle"
        {...attributes}
        {...listeners}
        style={{ 
          position: 'absolute',
          left: '8px',
          top: '50%',
          transform: 'translateY(-50%)',
          cursor: 'grab',
          padding: '4px',
          display: 'flex',
          alignItems: 'center',
          zIndex: 1
        }}
      >
        <BsGripVertical size={20} />
      </div>
      <div style={{ marginBottom: '0.5rem', marginLeft: '32px' }}>
        <strong>{contractFilename || 'untitled-contract.yaml'}</strong>
      </div>
      
      {/* Agents and Superagents - always visible */}
      {(agents.length > 0 || superagents.length > 0) && (
        <div style={{ marginBottom: '0.5rem', marginLeft: '32px', fontSize: '0.9em' }}>
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
      <div style={{ marginBottom: '0.5rem', marginLeft: '32px', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', gap: '0.25rem' }}>
          {simulations.map((s, i) => {
            return <Button key={i} id={contractId + ":" + s} variant={contractSims.has(s) ? "success" : "danger"} onClick={updateContractSim} size="sm">{s}</Button>
          })}
        </div>
        <div style={{ display: 'flex', gap: '0.25rem' }}>
          <a download={contractFilename || 'untitled-contract.yaml'} href={downloadLink}><Button size="sm" aria-label="Download"><BsDownload /></Button></a>
          <Button id={contractId} onClick={deleteContract} size="sm" variant="danger" aria-label="Delete"><BsTrash /></Button>
        </div>
      </div>
      
      {/* Error alert - always visible */}
      {contractError && <Alert variant="danger" style={{ marginBottom: '0.5rem', marginLeft: '32px' }}>{contractError}</Alert>}
  </Card>
}
