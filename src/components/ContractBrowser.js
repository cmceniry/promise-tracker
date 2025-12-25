import { useState, useEffect } from 'react';
import { Modal, Button, ListGroup, Spinner, Alert } from 'react-bootstrap';
import { BsFolder, BsFileEarmarkText, BsChevronRight, BsChevronDown, BsPlusLg } from 'react-icons/bs';

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
  onError,
  onDownloadContract,
  downloadedContractPaths,
  downloadingContracts
}) {
  const isExpanded = expandedPaths.has(path);
  const isLoading = loadingPaths.has(path);
  const children = loadedChildren.get(path) || [];
  const indent = level * 20;
  const isDownloaded = downloadedContractPaths.has(path);
  const isDownloading = downloadingContracts.has(path);
  const isDisabled = isDownloaded || isDownloading;

  const handleDirectoryClick = (e) => {
    e.stopPropagation();
    onToggleExpand(path);
  };

  const handleAddClick = (e) => {
    e.stopPropagation();
    if (!isDisabled) {
      onDownloadContract(path, entry.name);
    }
  };

  return (
    <>
      <ListGroup.Item
        action={entry.type === 'directory'}
        onClick={entry.type === 'directory' ? handleDirectoryClick : undefined}
        style={{
          cursor: entry.type === 'directory' ? 'pointer' : 'default',
          display: 'flex',
          alignItems: 'center',
          gap: '0.5rem',
          paddingLeft: `${8 + indent}px`,
          opacity: isDownloaded ? 0.6 : 1,
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
          <>
            {entry.type === 'contract' && (
              <BsFileEarmarkText size={20} style={{ flexShrink: 0 }} />
            )}
          </>
        )}
        <span style={{
          textDecoration: isDownloaded ? 'line-through' : 'none',
          flexGrow: 1
        }}>
          {entry.name}
        </span>
        {entry.type === 'contract' && (
          <Button
            variant="link"
            size="sm"
            onClick={handleAddClick}
            disabled={isDisabled}
            style={{
              padding: '0.25rem',
              color: isDisabled ? '#ccc' : '#007bff',
              cursor: isDisabled ? 'not-allowed' : 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
            title={isDownloaded ? 'Already downloaded' : isDownloading ? 'Downloading...' : 'Download contract'}
          >
            <BsPlusLg size={18} />
          </Button>
        )}
        {(isLoading || isDownloading) && (
          <Spinner animation="border" size="sm" style={{ marginLeft: '0.5rem' }} />
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
                  onError={onError}
                  onDownloadContract={onDownloadContract}
                  downloadedContractPaths={downloadedContractPaths}
                  downloadingContracts={downloadingContracts}
                />
              );
            })
          )}
        </>
      )}
    </>
  );
}

export default function ContractBrowser({ show, onHide, onSelectContract, downloadedContractPaths = new Set() }) {
  const [expandedPaths, setExpandedPaths] = useState(new Set());
  const [loadedChildren, setLoadedChildren] = useState(new Map());
  const [loadingPaths, setLoadingPaths] = useState(new Set());
  const [rootEntries, setRootEntries] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [downloadingContracts, setDownloadingContracts] = useState(new Set());

  // Fetch root directory listing when modal opens
  useEffect(() => {
    if (show) {
      setExpandedPaths(new Set());
      setLoadedChildren(new Map());
      setLoadingPaths(new Set());
      setDownloadingContracts(new Set());
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

  const handleDownloadContract = async (path, name) => {
    // Check if already downloading or downloaded
    if (downloadingContracts.has(path) || downloadedContractPaths.has(path)) {
      return;
    }

    // Mark contract as downloading
    setDownloadingContracts(prev => new Set(prev).add(path));
    setError(null);

    try {
      const contractContent = await fetchContract(path);
      // Extract name from path if name not provided
      const contractName = name || path.split('/').pop();
      onSelectContract(path, contractName, contractContent);
    } catch (err) {
      setError(prev => prev ? `${prev}; ${path}: ${err.message}` : `${path}: ${err.message}`);
    } finally {
      // Clear downloading state
      setDownloadingContracts(prev => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  };

  const handleClose = () => {
    setExpandedPaths(new Set());
    setLoadedChildren(new Map());
    setLoadingPaths(new Set());
    setRootEntries([]);
    setDownloadingContracts(new Set());
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
                    onError={setError}
                    onDownloadContract={handleDownloadContract}
                    downloadedContractPaths={downloadedContractPaths}
                    downloadingContracts={downloadingContracts}
                  />
                );
              })
            )}
          </ListGroup>
        )}
      </Modal.Body>
      <Modal.Footer>
        <Button variant="secondary" onClick={handleClose}>
          Close
        </Button>
      </Modal.Footer>
    </Modal>
  );
}

