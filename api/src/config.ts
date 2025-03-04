import { MainDatumArgs } from '../../src/input/mainArgs';
import { mergeConfigAndEnvIntoArgs } from '../../src/config/mergeConfigIntoArgs';

export function getConfig(): MainDatumArgs {
  // Start with default args
  const args: MainDatumArgs = {
    db: 'datum',
    // You can set defaults here, or let them come from config/env
  };

  // Merge config file and env vars
  mergeConfigAndEnvIntoArgs(args);

  return args;
}