import PouchDB from 'pouchdb';
import PouchDBFind from 'pouchdb-find';
import { EitherPayload } from '../../src/documentControl/DatumDocument';
import { getConfig } from './config';
import { setupDatumViews } from '../../src/views/setupDatumViews';

PouchDB.plugin(PouchDBFind);

let dbInstance: PouchDB.Database<EitherPayload> | null = null;
let syncHandler: PouchDB.Replication.Sync<EitherPayload> | null = null;

export async function getDb() {
  if (dbInstance) return dbInstance;

  const config = getConfig();

  // Create a separate database for the API
  const apiDb = new PouchDB<EitherPayload>(`${config.path || 'datum'}-api`);

  // Set up two-way sync with the main database
  const mainDb = new PouchDB<EitherPayload>(config.path || 'datum');

  // Set up continuous bidirectional sync
  syncHandler = apiDb.sync(mainDb, {
    live: true,
    retry: true
  }).on('error', function(err) {
    console.error('Sync error:', err);
  });

  // Ensure views are set up
  await setupDatumViews({ db: apiDb });

  dbInstance = apiDb;
  return apiDb;
}

// Add cleanup function for proper shutdown
export function closeDb() {
  if (syncHandler) {
    syncHandler.cancel();
  }
  if (dbInstance) {
    return dbInstance.close();
  }
  return Promise.resolve();
}