import { DatumData } from "../documentControl/DatumDocument";
import { interpolateFields } from "../utils/interpolateFields";
import { ExtractedAndFormatted, OutputProvider } from "./outputInterface";
import chalk from "chalk";
import stringify from "string.ify";
import { ACTIONS } from "./actions";

/**
 * Console-based implementation of the OutputProvider interface
 * Uses console.log for output display
 */
export class ConsoleOutputProvider implements OutputProvider {
  /**
   * Show a header line with action, human ID and document ID
   */
  showHeaderLine(formatted: ExtractedAndFormatted): void {
    console.log(
      [formatted.action, formatted.hid, formatted.id].filter(Boolean).join(" ")
    );
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
      console.log(infoLine);
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
    console.log(outputString);
  }

  /**
   * Show the complete document as a formatted string
   */
  showFullDocument(doc: any): void {
    console.log(stringify(doc));
  }

  /**
   * Show non-redundant document data
   */
  showNonRedundantData(formattedData: string): void {
    console.log(formattedData);
  }

  /**
   * Show a rename operation with before and after IDs
   */
  showRename(beforeId: string, afterId: string, action: ACTIONS): void {
    const color = this.getActionColor(action);
    const actionText = color.inverse(` ${action} `);
    console.log(
      actionText + color(beforeId) + " ⟶ " + chalk.green(afterId)
    );
  }

  /**
   * Get appropriate chalk color for an action
   */
  private getActionColor(action: ACTIONS) {
    const ACTION_CHALK = {
      [ACTIONS.Create]: chalk.green,
      [ACTIONS.Delete]: chalk.red,
      [ACTIONS.Exists]: chalk.yellow,
      [ACTIONS.Update]: chalk.cyan,
      [ACTIONS.OWrite]: chalk.blue,
      [ACTIONS.Rename]: chalk.cyan,
      [ACTIONS.NoDiff]: chalk.hex("#ffa500"),
      [ACTIONS.Failed]: chalk.red,
    };
    
    return ACTION_CHALK[action] || chalk;
  }
}