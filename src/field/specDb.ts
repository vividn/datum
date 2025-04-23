import PouchDB from "pouchdb";
import { EitherPayload } from "../documentControl/DatumDocument";
import { FieldSpec, getFieldSpec, setSpecCache } from "./mySpecs";
import { addDoc } from "../documentControl/addDoc";
import { updateDoc } from "../documentControl/updateDoc";

/**
 * Constructs the document ID for a field specification
 */
export function specDocId(field: string): string {
  return `SPEC:${field}`;
}

/**
 * Retrieve a field specification from the database
 * If not found, returns null (caller should fall back to hardcoded specs)
 */
export async function getSpecFromDb(
  db: PouchDB.Database<EitherPayload>,
  field: string,
): Promise<FieldSpec | null> {
  try {
    const doc = await db.get(specDocId(field)) as EitherPayload;
    const spec = doc.data as unknown as FieldSpec;
    
    // Update the cache with the spec from the database
    if (spec) {
      setSpecCache(field, spec);
    }
    
    return spec;
  } catch (error) {
    // Document not found or other error, return null
    return null;
  }
}

/**
 * Save a field specification to the database
 */
export async function saveSpecToDb(
  db: PouchDB.Database<EitherPayload>,
  field: string,
  spec: FieldSpec,
): Promise<void> {
  const docId = specDocId(field);
  
  try {
    // Try to get the existing document
    await db.get(docId);
    
    // Update existing document
    await updateDoc(db, docId, { 
      strategy: "update",
      data: spec
    });
  } catch (error: any) {
    // Document doesn't exist or other error, create new document
    if (error.status === 404) {
      await addDoc(db, {
        _id: docId,
        data: spec,
        meta: {}  // Include empty meta object so addDoc will add createTime, etc.
      });
    } else {
      throw error;
    }
  }
  
  // Update the cache with the new spec
  setSpecCache(field, spec);
}

/**
 * Load all specs from the database into the cache
 */
export async function loadAllSpecsFromDb(
  db: PouchDB.Database<EitherPayload>,
): Promise<Record<string, FieldSpec>> {
  try {
    const result = await db.allDocs({
      include_docs: true,
      startkey: 'SPEC:',
      endkey: 'SPEC:\ufff0',
    });
    
    const specs: Record<string, FieldSpec> = {};
    
    result.rows.forEach(row => {
      const doc = row.doc as EitherPayload;
      if (doc && doc._id.startsWith('SPEC:')) {
        const field = doc._id.substring(5); // Remove 'SPEC:' prefix
        const spec = doc.data as unknown as FieldSpec;
        specs[field] = spec;
        
        // Update the cache
        setSpecCache(field, spec);
      }
    });
    
    return specs;
  } catch (error) {
    console.error('Error loading specs from database:', error);
    return {};
  }
}

/**
 * Delete a field specification from the database
 */
export async function deleteSpecFromDb(
  db: PouchDB.Database<EitherPayload>,
  field: string,
): Promise<void> {
  const docId = specDocId(field);
  
  try {
    const doc = await db.get(docId);
    await db.remove(doc);
  } catch (error) {
    // Ignore if document doesn't exist
  }
}

/**
 * Simple validation for hex color strings
 */
export function isValidColor(color: string): boolean {
  return /^#[0-9A-Fa-f]{6}$/.test(color);
}

/**
 * Migration function to save all hardcoded specs to the database
 */
export async function migrateSpecsToDb(
  db: PouchDB.Database<EitherPayload>,
): Promise<void> {
  // Get all fields with specs from the hardcoded defaults
  const specCache = getFieldSpec();
  const fields = Object.keys(specCache);
  
  for (const field of fields) {
    const spec = getFieldSpec(field);
    if (Object.keys(spec).length > 0) {
      await saveSpecToDb(db, field, spec);
    }
  }
}