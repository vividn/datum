// TODO: Remove this file after proper specs are setup

export type FieldSpec = {
  kind?: "occur" | "start" | "switch";
  color?: string;
  y?: number; // 0 to 1
  height?: number; // 0 to 1
  zIndex?: number; // defaults to y
  states?: Record<string, Omit<FieldSpec, "states" | "kind">>;
  stateY?: number; // 0 to 1, relative to whole field
  stateHeight?: number; // 0 to 1, relative to whole field
};

export const FIELD_SPECS: Record<string, FieldSpec> = {
  address: {
    kind: "switch",
    y: 0,
    height: 0.001, // TODO: make this 0 once all bugs are worked out (inkscape transfroms svg in a weird way to png with 0 height blocks)
  },
  country: {
    kind: "switch",
    y: 0,
    height: 0.05,
    states: {
      germany: { color: "#303030" },
      usa: { color: "#0000ff" },
    },
  },
  city: {
    kind: "switch",
    y: 0.025,
    height: 0.025,
    states: {
      berlin: { color: "#300000" },
    },
  },
  environment: {
    kind: "switch",
    y: 0.05,
    height: 0.55,
    states: {
      home: { color: "#141414" },
      thome: { color: "#3c3c1e" },
      varya_home: { color: "#281414" },
      vnhome: { color: "#18211f" },
      inside: { color: "#5a5a5a" },
      outside: { color: "#87a366" },
      covered: { color: "#5a6e4b" },
      balcony: { color: "#4f6b2e" },
      porch: { color: "#45592e" },
      terrace: { color: "#4b593b" },
      courtyard: { color: "#5b8301" },
      cellar: { color: "#280f28" },
      office: { color: "#784646" },
      tent: { color: "#a16e42" },
      bicycle: { color: "#e2e564" },
      scooter: { color: "#e5b962" },
      car: { color: "#880e4f" },
      cart: { color: "#850f38" },
      bus: { color: "#bc386e" },
      train: { color: "#f06292" },
      tram: { color: "#f096c8" },
      airplane: { color: "#c8c8c8" },
      boat: { color: "#71c0d6" },
      boat_outside: { color: "#71c0d6" },
      boat_inside: { color: "#5ab0c7" },
      gondola: { color: "#785082" },
      chairlift: { color: "#6e5a96" },
    },
  },
  festival: {
    kind: "switch",
    y: 0.03,
    height: 0.97,
    states: {
      goldmund: { color: "#c49531" },
    },
  },
  body: {
    kind: "switch",
    y: 0.6,
    height: 0.4,
    zIndex: 0.1,
    states: {
      sit: { color: "#7299a6" },
      vertical: { color: "#e6dec1" },
      bed: { color: "#233791" },
      sofa: { color: "#8c72a6" },
      stretch: { color: "#d93494" },
      lift: { color: "#f6ff00" },
      swim: { color: "#0000ff" },
      pedal: { color: "#9bc83c" },
      run: { color: "#ff6e19" },
      yoga: { color: "#460087" },
      pingpong: { color: "#3c614c" },
      snowboard: { color: "#d2d2d2" },
      iceskate: { color: "#80c8d9" },
    },
  },
  sleep: {
    kind: "start",
    y: 0.25,
    height: 0.55,
    color: "#142878",
  },
  poop: {
    kind: "occur",
    color: "#422501",
    y: 0.85,
  },
  pee: {
    kind: "occur",
    color: "#bbaa00",
    y: 0.85,
  },
  therapy: {
    kind: "start",
    y: 0.25,
    height: 0.65,
    color: "#3d5cdc",
  },
  project: {
    kind: "switch",
    y: 0.15,
    height: 0.3,
    states: {
      berlin: { color: "#969600" },
      german: { color: "#ffce00" },
      portuguese: { color: "#009c3b" },
      spanish: { color: "#aa151b" },
      swedish: { color: "#00559b" },
      danish: { color: "#ea899a" },
      dutch: { color: "#1e4785" },
      russian: { color: "#de4c8a" },
      czech: { color: "#e6d690" },
      draw: { color: "#e1e1c8" },
      data: { color: "#791d8c" },
      datum: { color: "#650e78" },
      piano: { color: "#323232" },
      cello: { color: "#7a4300" },
      ukulele: { color: "#ffd6a8" },
      guitar: { color: "#ffd6a8" },
      linux: { color: "#3c8497" },
      nixos: { color: "#506e87" },
      household: { color: "#6e0000" },
      leary: { color: "#afffaf" },
      cook: { color: "#ff6464" },
      finances: { color: "#005120" },
      money: { color: "#005120" },
      taxes: { color: "#007838" },
      social: { color: "#000096" },
      inbox: { color: "#7f7f7f" },
      email: { color: "#7f7fa0" },
      tasks: { color: "#7fa0a0" },
      slack: { color: "#a07f7f" },
      smartwatch: { color: "#64dc8c" },
      interview: { color: "#bcff49" },
      crypto: { color: "#9682ef" },
      meeting: { color: "#000000" },
      backend: { color: "#84f0bd" },
      frontend: { color: "#c04ec2" },
      research: { color: "#2746b8" },
      infrastructure: { color: "#935202" },
      training: { color: "#d7d1ff" },
      review: { color: "#d5e640" },
      mastercard: { color: "#eb001b" },
      ethcontracts: { color: "#86eb3e" },
      algorand: { color: "#d10d31" },
      ethereum: { color: "#343474" },
      dependencies: { color: "#a47d90" },
      shipping_api: { color: "#633e44" },
      travel_prep: { color: "#ff00af" },
      documentation: { color: "#d2b4a3" },
      hiring: { color: "#42b246" },
      onboarding: { color: "#3cbe50" },
      housing: { color: "#45302a" },
      nvim: { color: "#b1d16d" },
      svelte: { color: "#ff3e00" },
    },
  },
  alarm: { kind: "occur", color: "#ffff00", y: 0.5 },
  meditate: { kind: "start", color: "#590ca0", y: 0.5, height: 0.2 },
  consume: {
    kind: "switch",
    y: 0.35,
    height: 0.1,
    states: {
      reddit: { color: "#91c8ff" },
      youtube: { color: "#ff0000" },
      tv: { color: "#c80000" },
      movie: { color: "#aa1b52" },
      nonfiction: { color: "#14c95c" },
      fiction: { color: "#467013" },
      erotic_fiction: { color: "#5f7013" },
      videogame: { color: "#b070ff" },
      podcast: { color: "#f4842d" },
      comics: { color: "#646464" },
      jigsaw: { color: "#1e1eff" },
      instagram: { color: "#b22463" },
      tinder: { color: "#ff655b" },
      okcupid: { color: "#ed5abe" },
      wakingup: { color: "#2866a7" },
      wikipedia: { color: "#f8fcff" },
    },
  },
  sunlamp: { kind: "start", color: "#c8c8ff", y: 0.65, height: 0.05 },
  contacts: { kind: "start", color: "#a1e6e4", y: 0.67, height: 0.02 },
  shower: { kind: "start", color: "#009bff", y: 0.6, height: 0.1 },
  bath: { kind: "start", color: "#00648c", y: 0.8, height: 0.2 },
  shave: { kind: "occur", color: "#a62505", y: 0.72, height: 0.05 },
  trim: { kind: "occur", color: "#712000", y: 0.9, height: 0.05 },

  caffeine: { kind: "occur", color: "#643200", y: 0.75 },
  alcohol: { color: "#00ffff", y: 0.75 },
  cannabis: { color: "#00c800", y: 0.75 },
} as const;

// Store the hardcoded specs in memory, for use by getFieldSpec and specCache
let specCache: Record<string, FieldSpec> = { ...FIELD_SPECS };

/**
 * Get a field specification, either from the cache or the hardcoded defaults
 */
export function getFieldSpec(field?: string): FieldSpec {
  if (!field) return {};
  return specCache[field] ?? {};
}

/**
 * Set the spec cache for a field
 */
export function setSpecCache(field: string, spec: FieldSpec): void {
  specCache[field] = spec;
}

/**
 * Reset the spec cache to the hardcoded defaults
 */
export function resetSpecCache(): void {
  specCache = { ...FIELD_SPECS };
}

/**
 * Get the entire spec cache
 */
export function getSpecCache(): Record<string, FieldSpec> {
  return { ...specCache };
}