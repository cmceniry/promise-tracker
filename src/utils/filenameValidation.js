/**
 * Validates a contract filename according to the rules:
 * - Must only contain lowercase a-z, digits 0-9, forward slash (/)
 * - Must end with ".yaml"
 * - Cannot start with "/"
 * - Cannot be empty
 * 
 * @param {string} filename - The filename to validate
 * @returns {string|null} - Error message if invalid, null if valid
 */
export function validateFilename(filename) {
  if (!filename || typeof filename !== 'string') {
    return 'Filename cannot be empty';
  }

  const trimmed = filename.trim();
  
  if (trimmed === '') {
    return 'Filename cannot be empty';
  }

  // Cannot start with "/"
  if (trimmed.startsWith('/')) {
    return 'Filename cannot start with "/"';
  }

  // Must end with ".yaml"
  if (!trimmed.endsWith('.yaml')) {
    return 'Filename must end with ".yaml"';
  }

  // Must only contain lowercase a-z, 0-9, forward slash (/), and the ".yaml" extension
  // Check all characters except the last 5 (which should be ".yaml")
  const namePart = trimmed.slice(0, -5);
  
  // Allow empty name part (just ".yaml" is technically valid per rules, but we'll reject it)
  if (namePart === '') {
    return 'Filename must have a name before ".yaml"';
  }

  // Check each character in the name part
  for (let i = 0; i < namePart.length; i++) {
    const char = namePart[i];
    const isLowercase = char >= 'a' && char <= 'z';
    const isDigit = char >= '0' && char <= '9';
    const isSlash = char === '/';
    
    if (!isLowercase && !isDigit && !isSlash) {
      return `Filename contains invalid character "${char}". Only lowercase letters (a-z), digits (0-9), and forward slash (/) are allowed.`;
    }
  }

  return null; // Valid
}

/**
 * Generates a random 8-character lowercase alphabetic filename with ".yaml" suffix
 * 
 * @returns {string} - A random filename like "abcdefgh.yaml"
 */
export function generateRandomFilename() {
  const chars = 'abcdefghijklmnopqrstuvwxyz';
  let result = '';
  for (let i = 0; i < 8; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result + '.yaml';
}

/**
 * Generates a unique random filename by checking against existing contracts
 * 
 * @param {Array} existingContracts - Array of contract objects with filename property
 * @param {number} maxAttempts - Maximum number of attempts to generate unique name (default: 100)
 * @returns {string} - A unique random filename
 */
export function generateUniqueRandomFilename(existingContracts, maxAttempts = 100) {
  const existingFilenames = new Set(
    existingContracts
      .map(c => c.filename)
      .filter(f => f && f.trim() !== '')
  );

  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    const filename = generateRandomFilename();
    if (!existingFilenames.has(filename)) {
      return filename;
    }
  }

  // If we've exhausted attempts, append a timestamp to make it unique
  const baseName = generateRandomFilename().slice(0, -5); // Remove .yaml
  const timestamp = Date.now().toString(36); // Base36 timestamp
  return `${baseName}${timestamp.slice(-4)}.yaml`; // Use last 4 chars of timestamp
}

/**
 * Normalizes a filename by trimming and converting to lowercase (if needed)
 * Note: We don't auto-lowercase since user might want mixed case in directory names
 * But we do trim whitespace
 * 
 * @param {string} filename - The filename to normalize
 * @returns {string} - Normalized filename
 */
export function normalizeFilename(filename) {
  if (!filename || typeof filename !== 'string') {
    return '';
  }
  return filename.trim();
}
