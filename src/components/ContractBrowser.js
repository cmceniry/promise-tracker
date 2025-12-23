import { useState, useEffect } from 'react';
import { Modal, Button, ListGroup, Breadcrumb, Spinner, Alert } from 'react-bootstrap';
import { BsFolder, BsFileEarmarkText } from 'react-icons/bs';

// API utility functions
const getApiBaseUrl = () => {
  return process.env.REACT_APP_API_URL || 'http://localhost:8080';
};

const fetchDirectoryListing = async (path = null) => {
  const baseUrl = getApiBaseUrl();
  let url = `${baseUrl}/contracts`;
  
  if (path) {
    // URL encode each path segment
    const encodedPath = path.split('/')
      .filter(segment => segment.length > 0)
      .map(segment => encodeURIComponent(segment))
      .join('/');
    url = `${baseUrl}/contracts/${encodedPath}`;
  }
  
  const response = await fetch(url, {
    headers: {
      'Accept': 'application/json',
    },
  });
  
  if (!response.ok) {
    throw new Error(`Failed to fetch directory: ${response.status} ${response.statusText}`);
  }
  
  return await response.json();
};

const fetchContract = async (contractId) => {
  const baseUrl = getApiBaseUrl();
  // URL encode each path segment
  const encodedPath = contractId.split('/')
    .filter(segment => segment.length > 0)
    .map(segment => encodeURIComponent(segment))
    .join('/');
  const url = `${baseUrl}/contracts/${encodedPath}`;
  
  const response = await fetch(url, {
    headers: {
      'Accept': 'application/x-yaml',
    },
  });
  
  if (!response.ok) {
    throw new Error(`Failed to fetch contract: ${response.status} ${response.statusText}`);
  }
  
  return await response.text();
};

export default function ContractBrowser({ show, onHide, onSelectContract }) {
  const [currentPath, setCurrentPath] = useState(null);
  const [entries, setEntries] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  // Fetch directory listing when modal opens or path changes
  useEffect(() => {
    if (show) {
      setCurrentPath(null);
      setError(null);
      loadDirectory(null);
    }
  }, [show]);

  const loadDirectory = async (path) => {
    setLoading(true);
    setError(null);
    try {
      const listing = await fetchDirectoryListing(path);
      setEntries(listing);
      setCurrentPath(path);
    } catch (err) {
      setError(err.message);
      setEntries([]);
    } finally {
      setLoading(false);
    }
  };

  const handleEntryClick = async (entry) => {
    if (entry.type === 'directory') {
      // Navigate into directory
      const newPath = currentPath 
        ? `${currentPath}/${entry.name}`
        : entry.name;
      await loadDirectory(newPath);
    } else if (entry.type === 'contract') {
      // Load the contract
      const contractId = currentPath
        ? `${currentPath}/${entry.name}`
        : entry.name;
      
      setLoading(true);
      setError(null);
      try {
        const contractContent = await fetchContract(contractId);
        onSelectContract(contractId, entry.name, contractContent);
        onHide();
      } catch (err) {
        setError(err.message);
      } finally {
        setLoading(false);
      }
    }
  };

  const handleBreadcrumbClick = (path) => {
    loadDirectory(path);
  };

  const getBreadcrumbs = () => {
    const breadcrumbs = [{ path: null, name: 'contracts' }];
    
    if (currentPath) {
      const segments = currentPath.split('/');
      let accumulatedPath = '';
      
      segments.forEach((segment, index) => {
        accumulatedPath = index === 0 
          ? segment 
          : `${accumulatedPath}/${segment}`;
        breadcrumbs.push({
          path: accumulatedPath,
          name: segment,
        });
      });
    }
    
    return breadcrumbs;
  };

  const handleClose = () => {
    setCurrentPath(null);
    setEntries([]);
    setError(null);
    onHide();
  };

  return (
    <Modal show={show} onHide={handleClose} size="lg">
      <Modal.Header closeButton>
        <Modal.Title>Load Contract from API</Modal.Title>
      </Modal.Header>
      <Modal.Body style={{ maxHeight: '60vh', overflowY: 'auto' }}>
        {error && (
          <Alert variant="danger" dismissible onClose={() => setError(null)}>
            {error}
          </Alert>
        )}
        
        <Breadcrumb>
          {getBreadcrumbs().map((crumb, index) => (
            <Breadcrumb.Item
              key={index}
              active={index === getBreadcrumbs().length - 1}
              onClick={() => {
                if (index < getBreadcrumbs().length - 1) {
                  handleBreadcrumbClick(crumb.path);
                }
              }}
              style={{
                cursor: index < getBreadcrumbs().length - 1 ? 'pointer' : 'default',
              }}
            >
              {crumb.name || 'contracts'}
            </Breadcrumb.Item>
          ))}
        </Breadcrumb>

        {loading ? (
          <div style={{ textAlign: 'center', padding: '2rem' }}>
            <Spinner animation="border" role="status">
              <span className="visually-hidden">Loading...</span>
            </Spinner>
          </div>
        ) : (
          <ListGroup>
            {entries.length === 0 ? (
              <ListGroup.Item>No contracts or directories found.</ListGroup.Item>
            ) : (
              entries.map((entry, index) => (
                <ListGroup.Item
                  key={index}
                  action
                  onClick={() => handleEntryClick(entry)}
                  style={{
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '0.5rem',
                  }}
                >
                  {entry.type === 'directory' ? (
                    <BsFolder size={20} />
                  ) : (
                    <BsFileEarmarkText size={20} />
                  )}
                  <span>{entry.name}</span>
                </ListGroup.Item>
              ))
            )}
          </ListGroup>
        )}
      </Modal.Body>
      <Modal.Footer>
        <Button variant="secondary" onClick={handleClose}>
          Cancel
        </Button>
      </Modal.Footer>
    </Modal>
  );
}

