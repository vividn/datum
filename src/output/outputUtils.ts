import chalk from "chalk";

/**
 * Output utilities for displaying messages in different contexts
 * This allows for abstraction of output to support browser-compatibility
 */

/**
 * Console-like interface for output functions
 */
export interface OutputInterface {
  log: (message: string) => void;
  info: (message: string) => void;
  warn: (message: string) => void;
  error: (message: string) => void;
}

/**
 * Default console implementation that uses the global console object
 * This ensures that when console.log is mocked in tests, the mocks
 * will be used correctly
 */
export const consoleOutput: OutputInterface = {
  log: (message: string) => console.log(message),
  info: (message: string) => console.info(message),
  warn: (message: string) => console.warn(message),
  error: (message: string) => console.error(message),
};

/**
 * Creates a string-returning output interface for browser environments
 * @returns An output interface that returns all values as strings
 */
export function createStringOutput(): OutputInterface & { messages: string[] } {
  const messages: string[] = [];

  return {
    log: (message: string) => {
      messages.push(message);
    },
    info: (message: string) => {
      messages.push(message);
    },
    warn: (message: string) => {
      // Make sure the colored message is not exactly the same as the input
      const coloredMessage = chalk.yellow("WARNING: " + message);
      messages.push(coloredMessage);
    },
    error: (message: string) => {
      // Make sure the colored message is not exactly the same as the input
      const coloredMessage = chalk.red("ERROR: " + message);
      messages.push(coloredMessage);
    },
    messages,
  };
}
