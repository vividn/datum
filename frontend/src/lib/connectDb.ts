import type PouchDB from 'pouchdb';

interface ConnectDbArgs {
  host?: string;
  db?: string;
  user?: string;
  password?: string;
}

export async function connectDb(args: ConnectDbArgs = {}): Promise<PouchDB.Database> {
  let PouchDBInstance;

  // Different import approach based on environment
  if (typeof window === 'undefined') {
    // Server-side (Node.js)
    try {
      PouchDBInstance = require('pouchdb');
      const memoryAdapter = require('pouchdb-adapter-memory');
      PouchDBInstance.plugin(memoryAdapter);
    } catch (error) {
      console.error('Failed to load PouchDB in Node environment:', error);
      throw new Error('Failed to initialize PouchDB: ' + (error as Error).message);
    }
  } else {
    // Client-side (browser)
    try {
      const [pouchModule, memoryModule] = await Promise.all([
        import('pouchdb'),
        import('pouchdb-adapter-memory')
      ]);

      PouchDBInstance = pouchModule.default;
      PouchDBInstance.plugin(memoryModule.default);
    } catch (error) {
      console.error('Failed to load PouchDB in browser environment:', error);
      throw new Error('Failed to initialize PouchDB: ' + (error as Error).message);
    }
  }

  const {
    host = process.env.NEXT_PUBLIC_COUCH_HOST,
    db = "datum",
    user = process.env.NEXT_PUBLIC_COUCH_USER,
    password = process.env.NEXT_PUBLIC_COUCH_PASSWORD
  } = args;

  if (!host) {
    throw new Error('Database host not configured');
  }

  // Determine protocol based on host (localhost uses http, others use https)
  const protocol = host.includes("localhost") ? "http" : "http";
  const fullDatabaseName = `${protocol}://${host}`;

  console.log(`Connecting to database at ${fullDatabaseName}`); // for debugging

  return new PouchDBInstance(fullDatabaseName, {
    auth: {
      username: user,
      password: password,
    },
  });
}