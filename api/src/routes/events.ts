import { Router, Request, Response } from 'express';
import { getDb } from '../db';
import { addDoc } from '../../../src/documentControl/addDoc';
import { updateDoc } from '../../../src/documentControl/updateDoc';
import { deleteDoc } from '../../../src/documentControl/deleteDoc';
import { quickId } from '../../../src/ids/quickId';
import { DatumData } from '../../../src/documentControl/DatumDocument';
import { MainDatumArgs } from '../../../src/input/mainArgs';
import PouchDB from 'pouchdb';

const router = Router();

// Type guard to check if db is a PouchDB database
function isDatabase(db: any): db is PouchDB.Database {
  return db && typeof db.put === 'function' && typeof db.get === 'function';
}

// Get events for a date range
router.get('/events/:startDate', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    const startDate = req.params.startDate;
    const endDate = req.query.endDate as string || startDate;

    const result = await db.query('datum/timingView', {
      startkey: startDate,
      endkey: endDate + '\ufff0',
      include_docs: true
    });

    res.json(result.rows);
  } catch (error) {
    console.error('Error fetching events:', error);
    res.status(500).json({ error: 'Failed to fetch events' });
  }
});

// Create new event
router.post('/events', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    const result = await addDoc({
      db: isDatabase(db) ? db : db as any, // Type assertion to avoid never type
      payload: req.body,
      outputArgs: {}
    });
    res.status(201).json(result);
  } catch (error) {
    console.error('Error creating event:', error);
    res.status(500).json({ error: 'Failed to create event' });
  }
});

// Update latest event
router.patch('/events/latest/:field?', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    const quickIdStr = req.params.field ? `@field=${req.params.field}` : '@';

    const id = await quickId(quickIdStr, {
      db: isDatabase(db) ? db : db as any, // Type assertion to avoid never type
      outputArgs: {}
    } as MainDatumArgs);

    if (!id) {
      res.status(404).json({ error: 'No matching event found' });
      return;
    }

    const result = await updateDoc({
      db: isDatabase(db) ? db : db as any, // Type assertion to avoid never type
      id: String(id), // Ensure id is a string
      payload: req.body,
      outputArgs: {}
    });
    res.json(result);
  } catch (error) {
    console.error('Error updating latest event:', error);
    res.status(500).json({ error: 'Failed to update latest event' });
  }
});

// Update event by ID
router.patch('/events/:id', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    const result = await updateDoc({
      db: isDatabase(db) ? db : db as any, // Type assertion to avoid never type
      id: String(req.params.id), // Ensure id is a string
      payload: req.body,
      outputArgs: {}
    });
    res.json(result);
  } catch (error) {
    console.error('Error updating event:', error);
    res.status(500).json({ error: 'Failed to update event' });
  }
});

// Delete latest event
router.delete('/events/latest/:field?', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    const quickIdStr = req.params.field ? `@field=${req.params.field}` : '@';

    const id = await quickId(quickIdStr, {
      db: isDatabase(db) ? db : db as any,
      outputArgs: {}
    } as MainDatumArgs);

    if (!id) {
      res.status(404).json({ error: 'No matching event found' });
      return;
    }

    await deleteDoc({
      db: isDatabase(db) ? db : db as any,
      id: String(id),
      outputArgs: {}
    });
    res.status(204).send();
  } catch (error) {
    console.error('Error deleting latest event:', error);
    res.status(500).json({ error: 'Failed to delete latest event' });
  }
});

// Delete event
router.delete('/events/:id', async (req: Request, res: Response) => {
  try {
    const db = await getDb();
    await deleteDoc({
      db: isDatabase(db) ? db : db as any, // Type assertion to avoid never type
      id: String(req.params.id), // Ensure id is a string
      outputArgs: {}
    });
    res.status(204).send();
  } catch (error) {
    console.error('Error deleting event:', error);
    res.status(500).json({ error: 'Failed to delete event' });
  }
});

export const eventsRouter = router;