import { ArgumentParser } from "argparse";
import chalk from "chalk";
import prompts from "prompts";
import { connectDb } from "../auth/connectDb";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { outputArgs } from "../input/outputArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { FieldSpec, getFieldSpec } from "../field/mySpecs";
import { loadAllSpecsFromDb, saveSpecToDb } from "../field/specDb";
import {
  convertNameToColor,
  generateColorVariations,
  generateRandomColor,
} from "../field/colorUtils";
import { getContrastTextColor } from "../utils/colorUtils";

export const specArgs = new ArgumentParser({
  add_help: false,
});

specArgs.add_argument("field", {
  help: "the field to show or edit spec for",
  nargs: "?",
});

specArgs.add_argument("--color", {
  help: "set color for the field or state (can be name or hex)",
});

specArgs.add_argument("--kind", {
  help: "set kind for the field (occur, start, or switch)",
  choices: ["occur", "start", "switch"],
});

specArgs.add_argument("--y", {
  help: "set y position (0-1) for the field",
  type: "float",
});

specArgs.add_argument("--height", {
  help: "set height (0-1) for the field",
  type: "float",
});

specArgs.add_argument("--migrate", {
  help: "migrate all specs from hardcoded to database",
  action: "store_true",
});

export const specCmdArgs = new ArgumentParser({
  description: "manage field specs",
  parents: [specArgs, dbArgs, outputArgs],
});

export type SpecCmdArgs = MainDatumArgs & {
  field?: string;
  color?: string;
  kind?: "occur" | "start" | "switch";
  y?: number;
  height?: number;
  migrate?: boolean;
};

// Helper function to display a color sample in the terminal
function displayColorSample(color: string, label?: string): void {
  const textColor = getContrastTextColor(color);
  const colorBg = chalk.bgHex(color).hex(textColor);
  console.log(colorBg(` ${label || color} `));
}

// Helper function to get field:state from input
function parseFieldAndState(fieldInput: string): {
  field: string;
  state?: string;
} {
  const [field, state] = fieldInput.split(":");
  return { field, state };
}

// Helper function to ask for spec properties interactively
async function promptForSpec(initialSpec: FieldSpec = {}): Promise<FieldSpec> {
  const kindChoices = [
    { title: "occur", value: "occur", description: "Point data (events)" },
    { title: "start", value: "start", description: "Block data with duration" },
    { title: "switch", value: "switch", description: "State changes" },
  ];

  const colorChoices = [
    "red",
    "blue",
    "green",
    "yellow",
    "purple",
    "orange",
    "cyan",
    "pink",
    "brown",
    "gray",
    "black",
    "white",
  ];

  const kindResponse = await prompts({
    type: "select",
    name: "kind",
    message: "Select field kind",
    choices: kindChoices,
    initial: initialSpec.kind
      ? kindChoices.findIndex((c) => c.value === initialSpec.kind)
      : 0,
  });

  const yResponse = await prompts({
    type: "number",
    name: "y",
    message: "Y position (0-1)",
    initial: initialSpec.y !== undefined ? initialSpec.y : 0.5,
    min: 0,
    max: 1,
  });

  // Height is required for start and switch, optional for occur
  let heightResponse = { height: initialSpec.height };
  if (kindResponse.kind === "start" || kindResponse.kind === "switch") {
    heightResponse = await prompts({
      type: "number",
      name: "height",
      message: "Height (0-1)",
      initial: initialSpec.height !== undefined ? initialSpec.height : 0.1,
      min: 0.001,
      max: 1,
    });
  }

  // Color selection
  const colorTypeResponse = await prompts({
    type: "select",
    name: "colorType",
    message: "How would you like to set the color?",
    choices: [
      { title: "Choose from common colors", value: "common" },
      { title: "Enter hex code", value: "hex" },
      { title: "Enter color name", value: "name" },
      { title: "Generate random color", value: "random" },
    ],
  });

  let finalColor = initialSpec.color || "#000000";

  if (colorTypeResponse.colorType === "common") {
    const colorResponse = await prompts({
      type: "select",
      name: "color",
      message: "Select color",
      choices: colorChoices.map((c) => {
        const hex = convertNameToColor(c) || "#000000";
        return {
          title: c,
          value: hex,
          description: hex,
        };
      }),
    });
    finalColor = colorResponse.color;
  } else if (colorTypeResponse.colorType === "hex") {
    const hexResponse = await prompts({
      type: "text",
      name: "color",
      message: "Enter hex color code (e.g. #ff0000)",
      validate: (value) =>
        /^#[0-9A-Fa-f]{6}$/.test(value)
          ? true
          : "Please enter a valid hex color",
      initial: initialSpec.color || "#000000",
    });
    finalColor = hexResponse.color;
  } else if (colorTypeResponse.colorType === "name") {
    const nameResponse = await prompts({
      type: "text",
      name: "color",
      message: "Enter color name",
      validate: (value) => {
        const hex = convertNameToColor(value);
        return hex ? true : "Color name not recognized";
      },
    });
    finalColor = convertNameToColor(nameResponse.color) || "#000000";
  } else if (colorTypeResponse.colorType === "random") {
    // Generate several variations and let user choose
    const variations = [
      generateRandomColor([0, 60]), // Red-Yellow
      generateRandomColor([60, 180]), // Yellow-Cyan
      generateRandomColor([180, 300]), // Cyan-Magenta
      generateRandomColor([300, 360]), // Magenta-Red
    ];

    console.log("Color options:");
    variations.forEach((color, index) => {
      displayColorSample(color, `Option ${index + 1}`);
    });

    const variationResponse = await prompts({
      type: "select",
      name: "color",
      message: "Select color",
      choices: variations.map((c, i) => ({
        title: `Option ${i + 1}`,
        value: c,
        description: c,
      })),
    });

    finalColor = variationResponse.color;
  }

  // Show the final color
  console.log("\nSelected color:");
  displayColorSample(finalColor);

  const spec: FieldSpec = {
    kind: kindResponse.kind,
    y: yResponse.y,
    color: finalColor,
  };

  if (heightResponse.height !== undefined) {
    spec.height = heightResponse.height;
  }

  return spec;
}

// Helper to prompt for state-specific color
async function promptForStateColor(initialColor?: string): Promise<string> {
  const colorChoices = [
    "red",
    "blue",
    "green",
    "yellow",
    "purple",
    "orange",
    "cyan",
    "pink",
    "brown",
    "gray",
    "black",
    "white",
  ];

  const colorTypeResponse = await prompts({
    type: "select",
    name: "colorType",
    message: "How would you like to set the state color?",
    choices: [
      { title: "Choose from common colors", value: "common" },
      { title: "Enter hex code", value: "hex" },
      { title: "Enter color name", value: "name" },
      { title: "Generate random color", value: "random" },
    ],
  });

  let finalColor = initialColor || "#000000";

  if (colorTypeResponse.colorType === "common") {
    const colorResponse = await prompts({
      type: "select",
      name: "color",
      message: "Select color",
      choices: colorChoices.map((c) => {
        const hex = convertNameToColor(c) || "#000000";
        return {
          title: c,
          value: hex,
          description: hex,
        };
      }),
    });
    finalColor = colorResponse.color;
  } else if (colorTypeResponse.colorType === "hex") {
    const hexResponse = await prompts({
      type: "text",
      name: "color",
      message: "Enter hex color code (e.g. #ff0000)",
      validate: (value) =>
        /^#[0-9A-Fa-f]{6}$/.test(value)
          ? true
          : "Please enter a valid hex color",
      initial: initialColor || "#000000",
    });
    finalColor = hexResponse.color;
  } else if (colorTypeResponse.colorType === "name") {
    const nameResponse = await prompts({
      type: "text",
      name: "color",
      message: "Enter color name",
      validate: (value) => {
        const hex = convertNameToColor(value);
        return hex ? true : "Color name not recognized";
      },
    });
    finalColor = convertNameToColor(nameResponse.color) || "#000000";
  } else if (colorTypeResponse.colorType === "random") {
    // Generate several variations and let user choose
    const variations = [
      generateRandomColor([0, 60]), // Red-Yellow
      generateRandomColor([60, 180]), // Yellow-Cyan
      generateRandomColor([180, 300]), // Cyan-Magenta
      generateRandomColor([300, 360]), // Magenta-Red
    ];

    console.log("Color options:");
    variations.forEach((color, index) => {
      displayColorSample(color, `Option ${index + 1}`);
    });

    const variationResponse = await prompts({
      type: "select",
      name: "color",
      message: "Select color",
      choices: variations.map((c, i) => ({
        title: `Option ${i + 1}`,
        value: c,
        description: c,
      })),
    });

    finalColor = variationResponse.color;
  }

  // Show the final color
  console.log("\nSelected color:");
  displayColorSample(finalColor);

  return finalColor;
}

// Display field spec details in a pretty format
function displaySpec(fieldName: string, spec: FieldSpec): void {
  console.log(chalk.bold(`\nSpec for field: ${fieldName}`));
  console.log(`Kind: ${spec.kind || "unspecified"}`);

  if (spec.y !== undefined) {
    console.log(`Y: ${spec.y}`);
  }

  if (spec.height !== undefined) {
    console.log(`Height: ${spec.height}`);
  }

  if (spec.color) {
    console.log("Color:");
    displayColorSample(spec.color);
  }

  if (spec.states && Object.keys(spec.states).length > 0) {
    console.log(chalk.bold("\nStates:"));
    Object.entries(spec.states).forEach(([state, stateSpec]) => {
      console.log(`- ${state}:`);
      if (stateSpec.color) {
        displayColorSample(stateSpec.color, state);
      }

      if (stateSpec.y !== undefined) {
        console.log(`  Y: ${stateSpec.y}`);
      }

      if (stateSpec.height !== undefined) {
        console.log(`  Height: ${stateSpec.height}`);
      }
    });
  }
}

export async function specCmd(
  args: SpecCmdArgs | string | string[],
  preparsed?: Partial<SpecCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(specCmdArgs, args, preparsed);
  const db = connectDb(args);

  // First load all specs from the database into cache
  await loadAllSpecsFromDb(db);

  // Migration mode
  if (args.migrate) {
    console.log("Migrating field specs from hardcoded defaults to database...");
    const specCache = getFieldSpec();
    const fields = Object.keys(specCache);

    for (const field of fields) {
      const spec = getFieldSpec(field);
      if (Object.keys(spec).length > 0) {
        await saveSpecToDb(db, field, spec);
        console.log(`Migrated spec for field: ${field}`);
      }
    }
    console.log("Migration complete!");
    return;
  }

  // If no field specified, show usage
  if (!args.field) {
    specCmdArgs.print_help();
    return;
  }

  // Parse field and state
  const { field, state } = parseFieldAndState(args.field);

  // Get existing spec (from cache, which should have loaded from DB if available)
  const existingSpec = getFieldSpec(field);

  // Handle state-specific color update via command line
  if (state && args.color) {
    // Handle direct state color update
    if (!existingSpec.states) {
      existingSpec.states = {};
    }

    // Initialize state if it doesn't exist
    if (!existingSpec.states[state]) {
      existingSpec.states[state] = {};
    }

    // Convert color name to hex if needed
    const colorHex = convertNameToColor(args.color);
    if (!colorHex) {
      // If not a valid color name, generate a random color in that hue range
      const variations = generateColorVariations(args.color, 1);
      existingSpec.states[state].color = variations[0];
    } else {
      existingSpec.states[state].color = colorHex;
    }

    // Save to DB and update cache
    await saveSpecToDb(db, field, existingSpec);
    console.log(`Updated color for ${field}:${state}`);
    displayColorSample(
      existingSpec.states[state].color || "#000000",
      `${field}:${state}`,
    );
    return;
  }

  // Handle field spec update via command line arguments
  if (
    args.color ||
    args.kind ||
    args.y !== undefined ||
    args.height !== undefined
  ) {
    const updatedSpec: FieldSpec = { ...existingSpec };

    if (args.kind) {
      updatedSpec.kind = args.kind;
    }

    if (args.y !== undefined) {
      updatedSpec.y = args.y;
    }

    if (args.height !== undefined) {
      updatedSpec.height = args.height;
    }

    if (args.color) {
      // Convert color name to hex if needed
      const colorHex = convertNameToColor(args.color);
      if (!colorHex) {
        // If not a valid color name, generate a random color in that hue range
        const variations = generateColorVariations(args.color, 1);
        updatedSpec.color = variations[0];
      } else {
        updatedSpec.color = colorHex;
      }
    }

    // Save to DB and update cache
    await saveSpecToDb(db, field, updatedSpec);
    console.log(`Updated spec for field: ${field}`);
    displaySpec(field, updatedSpec);
    return;
  }

  // Interactive state-specific color update
  if (state) {
    console.log(`Setting color for state: ${field}:${state}`);

    if (!existingSpec.states) {
      existingSpec.states = {};
    }

    // Initialize state if it doesn't exist
    if (!existingSpec.states[state]) {
      existingSpec.states[state] = {};
    }

    // Prompt for color
    const stateColor = await promptForStateColor(
      existingSpec.states[state].color,
    );
    existingSpec.states[state].color = stateColor;

    // Save to DB and update cache
    await saveSpecToDb(db, field, existingSpec);
    console.log(`Updated color for ${field}:${state}`);
    displayColorSample(stateColor, `${field}:${state}`);
    return;
  }

  // If no updates provided, show existing spec
  if (Object.keys(existingSpec).length > 0) {
    displaySpec(field, existingSpec);
  }
  // If no existing spec, prompt to create one
  else {
    console.log(`No spec found for field: ${field}`);
    const createResponse = await prompts({
      type: "confirm",
      name: "create",
      message: "Would you like to create a spec for this field?",
      initial: true,
    });

    if (createResponse.create) {
      const newSpec = await promptForSpec();
      await saveSpecToDb(db, field, newSpec);
      console.log(`Created spec for field: ${field}`);
      displaySpec(field, newSpec);
    }
  }
}
