import chalk from "chalk";

/**
 * Output utilities for displaying messages in different contexts
 * This allows for abstraction of output to support browser-compatibility
 */

export type OutputFunction = (message: string) => void;

// Default output function that uses console.log
const defaultOutput: OutputFunction = (message: string) => {
  console.log(message);
};

/**
 * Utility for displaying info messages
 * @param message The message to display
 * @param output Optional output function (defaults to console.log)
 * @returns The formatted message string
 */
export function info(
  message: string,
  output: OutputFunction = defaultOutput,
): string {
  const formattedMessage = message;
  output(formattedMessage);
  return formattedMessage;
}

/**
 * Utility for displaying warning messages
 * @param message The message to display
 * @param output Optional output function (defaults to console.log)
 * @returns The formatted message string
 */
export function warn(
  message: string,
  output: OutputFunction = defaultOutput,
): string {
  const formattedMessage = chalk.yellow(message);
  output(formattedMessage);
  return formattedMessage;
}

/**
 * Utility for displaying error messages
 * @param message The message to display
 * @param output Optional output function (defaults to console.log)
 * @returns The formatted message string
 */
export function error(
  message: string,
  output: OutputFunction = defaultOutput,
): string {
  const formattedMessage = chalk.red(message);
  output(formattedMessage);
  return formattedMessage;
}

/**
 * Utility for displaying success messages
 * @param message The message to display
 * @param output Optional output function (defaults to console.log)
 * @returns The formatted message string
 */
export function success(
  message: string,
  output: OutputFunction = defaultOutput,
): string {
  const formattedMessage = chalk.green(message);
  output(formattedMessage);
  return formattedMessage;
}
