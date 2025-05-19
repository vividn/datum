import { DatumData } from "../documentControl/DatumDocument";
import { interpolateFields } from "../utils/interpolateFields";
import { ExtractedAndFormatted, OutputProvider } from "./outputInterface";
import { ACTIONS } from "./actions";

/**
 * Output types that can be returned by the browser output provider
 */
export enum BrowserOutputType {
  Header = "header",
  Info = "info",
  CustomFormat = "custom",
  FullDocument = "full",
  NonRedundantData = "nonRedundant",
  Rename = "rename",
  SVG = "svg",
  Error = "error",
  Warning = "warning",
}

/**
 * Result from browser output operation
 */
export type BrowserOutputResult = {
  type: BrowserOutputType;
  content: string | Record<string, any>;
};

/**
 * Browser-compatible implementation of the OutputProvider interface
 * Returns structured data instead of logging to console
 */
export class BrowserOutputProvider implements OutputProvider {
  private outputs: BrowserOutputResult[] = [];

  /**
   * Clear all collected outputs
   */
  clearOutputs(): void {
    this.outputs = [];
  }

  /**
   * Get all collected outputs
   */
  getOutputs(): BrowserOutputResult[] {
    return [...this.outputs];
  }

  /**
   * Show a header line with action, human ID and document ID
   */
  showHeaderLine(formatted: ExtractedAndFormatted): void {
    const headerContent = [formatted.action, formatted.hid, formatted.id]
      .filter(Boolean)
      .join(" ");
    
    this.outputs.push({
      type: BrowserOutputType.Header,
      content: headerContent
    });
  }

  /**
   * Show main information line with time, field, state and duration
   */
  showMainInfoLine(formatted: ExtractedAndFormatted): void {
    const infoLine = [
      formatted.time.occur,
      formatted.field,
      formatted.state,
      formatted.dur,
    ]
      .filter(Boolean)
      .join(" ");
    
    if (infoLine !== "") {
      this.outputs.push({
        type: BrowserOutputType.Info,
        content: infoLine
      });
    }
  }

  /**
   * Show custom formatted output based on a format string
   */
  showCustomFormat(data: DatumData, meta: any, formatString: string): void {
    const outputString = interpolateFields({ 
      data, 
      meta, 
      format: formatString 
    });
    
    this.outputs.push({
      type: BrowserOutputType.CustomFormat,
      content: outputString
    });
  }

  /**
   * Show the complete document as a formatted string
   */
  showFullDocument(doc: any): void {
    this.outputs.push({
      type: BrowserOutputType.FullDocument,
      content: doc
    });
  }

  /**
   * Show non-redundant document data
   */
  showNonRedundantData(formattedData: string): void {
    this.outputs.push({
      type: BrowserOutputType.NonRedundantData,
      content: formattedData
    });
  }

  /**
   * Show a rename operation with before and after IDs
   */
  showRename(beforeId: string, afterId: string, action: ACTIONS): void {
    this.outputs.push({
      type: BrowserOutputType.Rename,
      content: {
        action,
        beforeId,
        afterId
      }
    });
  }

  /**
   * Add an SVG output
   */
  addSvgOutput(svg: string): void {
    this.outputs.push({
      type: BrowserOutputType.SVG,
      content: svg
    });
  }

  /**
   * Add an error output
   */
  addError(error: Error | string): void {
    this.outputs.push({
      type: BrowserOutputType.Error,
      content: typeof error === 'string' ? error : error.message
    });
  }

  /**
   * Add a warning output
   */
  addWarning(warning: string): void {
    this.outputs.push({
      type: BrowserOutputType.Warning,
      content: warning
    });
  }
}