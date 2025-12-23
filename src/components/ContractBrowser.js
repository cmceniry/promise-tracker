import { useState, useEffect } from 'react';
import { Modal, Button, ListGroup, Spinner, Alert } from 'react-bootstrap';
import { BsFolder, BsFileEarmarkText, BsChevronRight, BsChevronDown } from 'react-icons/bs';

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

// Recursive TreeNode component
function TreeNode({ 
  entry, 
  path, 
  level, 
  expandedPaths, 
  loadedChildren, 
  loadingPaths,
  onToggleExpand,
  onSelectContract,
  onHide,
  onError
}) {
  const isExpanded = expandedPaths.has(path);
  const isLoading = loadingPaths.has(path);
  const children = loadedChildren.get(path) || [];
  const indent = level * 20;

  const handleDirectoryClick = (e) => {
    e.stopPropagation();
    onToggleExpand(path);
  };

  const handleContractClick = async (e) => {
    e.stopPropagation();
    try {
      const contractContent = await fetchContract(path);
      const name = entry.name;
      onSelectContract(path, name, contractContent);
      onHide();
    } catch (err) {
      onError(err.message);
    }
  };

  return (
    <>
      <ListGroup.Item
        action
        onClick={entry.type === 'directory' ? handleDirectoryClick : handleContractClick}
        style={{
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          gap: '0.5rem',
          paddingLeft: `${8 + indent}px`,
        }}
      >
        {entry.type === 'directory' ? (
          <>
            {isExpanded ? (
              <BsChevronDown size={16} style={{ flexShrink: 0 }} />
            ) : (
              <BsChevronRight size={16} style={{ flexShrink: 0 }} />
            )}
            <BsFolder size={20} style={{ flexShrink: 0 }} />
          </>
        ) : (
          <span style={{ width: '16px', display: 'inline-block', flexShrink: 0 }} />
        )}
        {entry.type === 'contract' && (
          <BsFileEarmarkText size={20} style={{ flexShrink: 0 }} />
        )}
        <span>{entry.name}</span>
        {isLoading && (
          <Spinner animation="border" size="sm" style={{ marginLeft: 'auto' }} />
        )}
      </ListGroup.Item>
      {entry.type === 'directory' && isExpanded && !isLoading && (
        <>
          {children.length === 0 ? (
            <ListGroup.Item style={{ paddingLeft: `${8 + indent + 20}px`, fontStyle: 'italic', color: '#666' }}>
              Empty directory
            </ListGroup.Item>
          ) : (
            children.map((child) => {
              const childPath = path ? `${path}/${child.name}` : child.name;
              return (
                <TreeNode
                  key={childPath}
                  entry={child}
                  path={childPath}
                  level={level + 1}
                  expandedPaths={expandedPaths}
                  loadedChildren={loadedChildren}
                  loadingPaths={loadingPaths}
                  onToggleExpand={onToggleExpand}
                  onSelectContract={onSelectContract}
                  onHide={onHide}
                  onError={onError}
                />
              );
            })
          )}
        </>
      )}
    </>
  );
}

export default function ContractBrowser({ show, onHide, onSelectContract }) {
  const [expandedPaths, setExpandedPaths] = useState(new Set());
  const [loadedChildren, setLoadedChildren] = useState(new Map());
  const [loadingPaths, setLoadingPaths] = useState(new Set());
  const [rootEntries, setRootEntries] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  // Fetch root directory listing when modal opens
  useEffect(() => {
    if (show) {
      setExpandedPaths(new Set());
      setLoadedChildren(new Map());
      setLoadingPaths(new Set());
      setError(null);
      loadRootDirectory();
    }
  }, [show]);

  const loadRootDirectory = async () => {
    setLoading(true);
    setError(null);
    try {
      const listing = await fetchDirectoryListing(null);
      setRootEntries(listing);
    } catch (err) {
      setError(err.message);
      setRootEntries([]);
    } finally {
      setLoading(false);
    }
  };

  const loadDirectory = async (path) => {
    // Check if already loaded
    if (loadedChildren.has(path)) {
      return;
    }

    setLoadingPaths(prev => new Set(prev).add(path));
    setError(null);
    try {
      const listing = await fetchDirectoryListing(path);
      setLoadedChildren(prev => new Map(prev).set(path, listing));
    } catch (err) {
      setError(err.message);
    } finally {
      setLoadingPaths(prev => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  };

  const handleToggleExpand = async (path) => {
    const isExpanded = expandedPaths.has(path);
    
    if (isExpanded) {
      // Collapse
      setExpandedPaths(prev => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    } else {
      // Expand - load children if not already loaded
      setExpandedPaths(prev => new Set(prev).add(path));
      await loadDirectory(path);
    }
  };

  const handleClose = () => {
    setExpandedPaths(new Set());
    setLoadedChildren(new Map());
    setLoadingPaths(new Set());
    setRootEntries([]);
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

        {loading ? (
          <div style={{ textAlign: 'center', padding: '2rem' }}>
            <Spinner animation="border" role="status">
              <span className="visually-hidden">Loading...</span>
            </Spinner>
          </div>
        ) : (
          <ListGroup>
            {rootEntries.length === 0 ? (
              <ListGroup.Item>No contracts or directories found.</ListGroup.Item>
            ) : (
              rootEntries.map((entry) => {
                const path = entry.name;
                return (
                  <TreeNode
                    key={path}
                    entry={entry}
                    path={path}
                    level={0}
                    expandedPaths={expandedPaths}
                    loadedChildren={loadedChildren}
                    loadingPaths={loadingPaths}
                    onToggleExpand={handleToggleExpand}
                    onSelectContract={onSelectContract}
                    onHide={onHide}
                    onError={setError}
                  />
                );
              })
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

