# Additional Notes for Commit 9: Browser-Compatible Database Connection

Based on further discussion with the project owner, here are detailed notes for the preferred approach to handling CouchDB authentication and sync:

## Token-based Authentication Approach

Instead of storing encrypted passwords, the implementation should:

1. **Use CouchDB Auth Cookies/Tokens**:
   - Authenticate with CouchDB to get an auth token/cookie
   - Store this token in a _local doc in the PouchDB database
   - This makes the token available to both browser and CLI clients

2. **Functions to Implement**:
   ```typescript
   // Authenticate with CouchDB and get auth token
   async function authenticate(url: string, username: string, password: string): Promise<string>
   
   // Store auth token in a _local doc
   async function storeAuthToken(db: PouchDB.Database, url: string, authToken: string): Promise<void>
   
   // Retrieve stored auth token
   async function getAuthToken(db: PouchDB.Database, url: string): Promise<string | null>
   
   // Clear stored auth token
   async function clearAuthToken(db: PouchDB.Database, url: string): Promise<void>
   ```

3. **Sync Configuration**:
   - Store minimal sync configuration in _local docs:
   ```typescript
   interface SyncConfig {
     url: string;
     continuous?: boolean;
     syncDirection?: 'push' | 'pull' | 'both';
   }
   ```

4. **Key Benefits**:
   - No need to store or encrypt passwords
   - Auth tokens have limited scope and lifetime
   - Storage in _local docs makes it available across platforms
   - Works seamlessly with PouchDB's existing sync capabilities

5. **Security Advantages**:
   - Follows the principle of least privilege
   - Removes the need for complex encryption
   - Uses standard CouchDB security mechanisms
   - Makes implementation simpler and more reliable

This approach should be implemented in the `/src/auth/syncManager.ts` module and integrated with the existing `connectDbBrowser.ts` file.