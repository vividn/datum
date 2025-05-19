export { ACTIONS } from "./actions";
export { OutputProvider, ExtractedAndFormatted, OutputOptions } from "./outputInterface";
export { ConsoleOutputProvider } from "./consoleOutput";
export { BrowserOutputProvider, BrowserOutputType } from "./browserOutput";

// Create and export default instances of output providers
import { ConsoleOutputProvider } from "./consoleOutput";
import { BrowserOutputProvider } from "./browserOutput";

export const consoleOutput = new ConsoleOutputProvider();
export const browserOutput = new BrowserOutputProvider();