// API utility function
const getApiBaseUrl = () => {
  return process.env.REACT_APP_API_URL || 'http://localhost:8080';
};

/**
 * Fetch contract content from the server API
 * @param {string} contractPath - The contract path/filename
 * @returns {Promise<string>} The contract content, or null if not found
 * @throws {Error} If there's a network error or server error
 */
export const fetchServerContract = async (contractPath) => {
  if (!contractPath || contractPath.trim() === '') {
    return null;
  }
  
  const baseUrl = getApiBaseUrl();
  // URL encode each path segment
  const encodedPath = contractPath.split('/')
    .filter(segment => segment.length > 0)
    .map(segment => encodeURIComponent(segment))
    .join('/');
  const url = `${baseUrl}/contracts/${encodedPath}`;
  
  try {
    const response = await fetch(url, {
      headers: {
        'Accept': 'application/x-yaml',
      },
    });
    
    if (response.status === 404) {
      return null; // Contract not found on server
    }
    
    if (!response.ok) {
      throw new Error(`Failed to fetch contract: ${response.status} ${response.statusText}`);
    }
    
    return await response.text();
  } catch (err) {
    // Re-throw network errors so caller can handle them
    throw err;
  }
};

/**
 * Normalize contract content for comparison
 * - Trims whitespace from start and end
 * - Normalizes line endings to \n
 * - Removes trailing whitespace from lines
 * @param {string} text - The contract text to normalize
 * @returns {string} Normalized text
 */
export const normalizeContent = (text) => {
  if (!text) {
    return '';
  }
  
  return text
    .split(/\r\n|\r|\n/) // Split by any line ending
    .map(line => line.trimEnd()) // Remove trailing whitespace from each line
    .join('\n') // Join with normalized line endings
    .trim(); // Remove leading/trailing whitespace
};

/**
 * Compare two contract texts (normalized)
 * @param {string} localText - Local contract text
 * @param {string} serverText - Server contract text
 * @returns {boolean} True if contracts are different, false if same
 */
export const compareContracts = (localText, serverText) => {
  const normalizedLocal = normalizeContent(localText || '');
  const normalizedServer = normalizeContent(serverText || '');
  return normalizedLocal !== normalizedServer;
};

/**
 * Check if local filename differs from server path
 * @param {string} localFilename - Current local filename
 * @param {string} serverPath - Original server path (if available)
 * @returns {boolean} True if filename differs from server path, false if same or no server path
 */
export const checkFilenameDiff = (localFilename, serverPath) => {
  if (!serverPath || !localFilename) {
    return false; // No server path to compare against, or no local filename
  }
  
  // Normalize both paths for comparison (trim and normalize)
  const normalizedLocal = (localFilename || '').trim();
  const normalizedServer = (serverPath || '').trim();
  
  return normalizedLocal !== normalizedServer;
};

