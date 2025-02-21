import { connectDb } from '../../src/auth/connectDb';
import { setupDatumViews } from '../../src/views/setupDatumViews';
import { EitherPayload } from '../../src/documentControl/DatumDocument';
import { getConfig } from './config';
import PouchDB from 'pouchdb';

let dbInstance: PouchDB.Database<EitherPayload> | null = null;

export async function getDb() {
  if (dbInstance) return dbInstance;

  const config = getConfig();
  const db = await connectDb(config);

  // Ensure views are set up
  await setupDatumViews({ db });

  dbInstance = db;
  return db;
}