import nano from 'nano';

interface DatumDoc {
  occurTime?: {
    utc: string;
  };
  field?: string;
}

interface DesignDoc {
  _id: string;
  _rev?: string;
  views: {
    by_hour: {
      map: string;
      reduce: string;
    };
  };
}

// Define map function as a raw string to avoid TypeScript parsing
const mapFunction = `
function(doc) {
  if (!doc.occurTime || !doc.field) return;
  const date = new Date(doc.occurTime.utc);
  const hour = date.getUTCHours();
  emit([doc.field, hour], 1);
}`.toString();

const designDoc: DesignDoc = {
  _id: '_design/datum',
  views: {
    'by_hour': {
      map: mapFunction,
      reduce: '_count'
    }
  }
};

const COUCH_URL = 'http://localhost:5984';
const DB_NAME = 'datum';

async function checkConnection() {
  const couch = nano(COUCH_URL);
  try {
    await couch.db.list();
    return true;
  } catch (error) {
    console.error('Failed to connect to CouchDB:', error.message);
    console.log('Please ensure CouchDB is running at', COUCH_URL);
    return false;
  }
}

async function ensureDatabase() {
  const couch = nano(COUCH_URL);
  try {
    const dbList = await couch.db.list();
    if (!dbList.includes(DB_NAME)) {
      console.log(`Creating database: ${DB_NAME}`);
      await couch.db.create(DB_NAME);
    }
    return true;
  } catch (error) {
    console.error('Failed to create/check database:', error.message);
    return false;
  }
}

async function deployView() {
  try {
    // Try to get existing design doc
    const existing = await db.get('_design/datum').catch(() => null);
    if (existing && '_rev' in existing) {
      designDoc._rev = existing._rev;
    }
    await db.insert(designDoc);
    console.log('View deployed successfully');
  } catch (error) {
    console.error('Failed to deploy view:', error);
  }
}

deployView();
