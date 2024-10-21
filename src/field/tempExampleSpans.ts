// TODO: Remove this file after proper specs are setup
export const DAYVIEW_SPANS: Record<string, [number, number]> = {
  environment: [0, 0.6],
  festival: [0.03, 1],
  body: [0.6, 1],
  sleep: [0.15, 0.85],
  therapy: [0.25, 0.65],
  project: [0.15, 0.45],
  consume: [0.35, 0.45],
  sex: [0.8, 0.95],
  contacts: [0.67, 0.69],
} as const;
