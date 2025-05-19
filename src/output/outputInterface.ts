import { DatumData } from "../documentControl/DatumDocument";
import { Show } from "../input/outputArgs";
import { ACTIONS } from "./actions";

/**
 * Represents formatted document data extracted for display
 */
export type ExtractedAndFormatted = {
  action: string;
  id?: string;
  hid?: string;
  time: {
    hybrid?: string;
    occur?: string;
    modify?: string;
    create?: string;
    none: undefined;
  };
  field?: string;
  state?: string;
  dur?: string;
};

/**
 * Interface for output providers that can render Datum output
 */
export interface OutputProvider {
  /**
   * Show a header line with action, human ID and document ID
   * @param formatted Extracted and formatted document data
   */
  showHeaderLine(formatted: ExtractedAndFormatted): void;

  /**
   * Show main information line with time, field, state and duration
   * @param formatted Extracted and formatted document data
   */
  showMainInfoLine(formatted: ExtractedAndFormatted): void;

  /**
   * Show custom formatted output based on a format string
   * @param data Document data
   * @param meta Document metadata
   * @param formatString Format string with field interpolation
   */
  showCustomFormat(data: DatumData, meta: any, formatString: string): void;

  /**
   * Show the complete document as a formatted string
   * @param doc Document to display
   */
  showFullDocument(doc: any): void;

  /**
   * Show non-redundant document data
   * @param formattedData Pre-formatted document data
   */
  showNonRedundantData(formattedData: string): void;

  /**
   * Show a rename operation with before and after IDs
   * @param beforeId Original document ID
   * @param afterId New document ID
   * @param action Action type
   */
  showRename(beforeId: string, afterId: string, action: ACTIONS): void;
}

/**
 * Options for output rendering
 */
export interface OutputOptions {
  show: Show;
  formatString?: string;
  outputProvider: OutputProvider;
}