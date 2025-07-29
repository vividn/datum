import { ArgumentParser } from "argparse";
import { DatumDocument } from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { timingView } from "../views/datumViews/timingView";
import { addDoc } from "../documentControl/addDoc";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { AIService } from "../ai/aiService";
import { occurredFields } from "../field/occurredFields";
import prompts from "prompts";
import { toDatumTime } from "../time/datumTime";
import { DateTime } from "luxon";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigAndEnvIntoArgs } from "../config/mergeConfigIntoArgs";
import dotenv from "dotenv";
import { fieldArgs } from "../input/fieldArgs";
import { outputArgs } from "../input/outputArgs";
import { connectDb } from "../auth/connectDb";

export const aiCmd = async (
  args: string[],
  namespace: MainDatumArgs,
): Promise<void> => {
  const parser = new ArgumentParser({
    prog: "datum ai",
    description: "Use AI to parse natural language input or analyze data",
    parents: [fieldArgs, outputArgs],
  });

  parser.add_argument("input", {
    help: "Natural language input to parse",
    nargs: "*",
  });

  parser.add_argument("--mode", {
    help: "AI operation mode",
    choices: ["parse", "insights", "predict", "explain", "all"],
    default: "parse",
  });

  parser.add_argument("--provider", {
    help: "AI provider to use",
    choices: ["claude", "openai"],
    default: "claude",
  });

  parser.add_argument("--api-key", {
    help: "AI API key (ANTHROPIC_API_KEY for Claude, OPENAI_API_KEY for OpenAI)",
  });

  parser.add_argument("--model", {
    help: "AI model to use",
    default: "claude-3-haiku-20240307",
  });

  parser.add_argument("-i", "--interactive", {
    help: "Interactively confirm AI interpretation",
    action: "store_true",
    default: false,
  });

  parser.add_argument("-q", "--question", {
    help: "Question to ask about your data (with --mode explain)",
  });

  parser.add_argument("-n", {
    help: "Number of documents to analyze",
    type: "int",
    default: 50,
  });

  const cmdArgs = parser.parse_args(args, namespace);
  
  // Load .env file
  dotenv.config();
  
  mergeConfigAndEnvIntoArgs(cmdArgs);
  const cArgs = cmdArgs;
  
  const db = connectDb(cArgs);
  const { n } = cArgs;
  
  const provider = cArgs.provider || "claude";
  const apiKey = cArgs.api_key || 
    (provider === "claude" ? process.env.ANTHROPIC_API_KEY : process.env.OPENAI_API_KEY);
  
  if (!apiKey) {
    const envVar = provider === "claude" ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY";
    throw new Error(`${provider} API key required. Set ${envVar} environment variable or pass --api-key`);
  }

  const aiService = new AIService({
    apiKey,
    provider,
    model: cArgs.model || (provider === "claude" ? "claude-3-haiku-20240307" : "gpt-4o-mini"),
    enableNLP: cArgs.mode !== "insights" && cArgs.mode !== "predict",
    enableInsights: cArgs.mode === "insights" || cArgs.mode === "all",
    enablePredictions: cArgs.mode === "predict" || cArgs.mode === "all",
  });

  if (cArgs.mode === "insights") {
    const viewResults = await viewMap({
      db,
      datumView: timingView,
      params: { limit: n || 100, include_docs: true, descending: true },
    });
    const documents = viewResults.rows.map(row => row.doc as DatumDocument).filter(Boolean);
    const insights = await aiService.generateInsights(documents, db);
    
    if (insights.length === 0) {
      console.log("No insights generated. Try adding more data first.");
      return;
    }

    console.log("\n🤖 AI Insights:\n");
    insights.forEach((insight, i) => {
      console.log(`${i + 1}. ${insight.type.toUpperCase()}: ${insight.description}`);
      if (insight.field) console.log(`   Field: ${insight.field}`);
      if (insight.fields) console.log(`   Fields: ${insight.fields.join(", ")}`);
      console.log(`   Confidence: ${(insight.confidence * 100).toFixed(0)}%`);
      if (insight.data) console.log(`   Data: ${JSON.stringify(insight.data)}`);
      console.log();
    });
    return;
  }

  if (cArgs.mode === "predict") {
    const fields = cArgs.field ? cArgs.field.split(",") : await occurredFields(db);
    const viewResults = await viewMap({
      db,
      datumView: timingView,
      params: { limit: n || 100, include_docs: true, descending: true },
    });
    const documents = viewResults.rows.map(row => row.doc as DatumDocument).filter(Boolean);
    const predictions = await aiService.generatePredictions(documents, fields);

    if (predictions.length === 0) {
      console.log("No predictions generated. Need more historical data.");
      return;
    }

    console.log("\n🔮 AI Predictions:\n");
    predictions.forEach((pred) => {
      console.log(`${pred.field}: ${JSON.stringify(pred.predictedValue)}`);
      console.log(`   Date: ${pred.date.utc}`);
      console.log(`   Confidence: ${(pred.confidence * 100).toFixed(0)}%`);
      console.log(`   Based on: ${pred.basedOn.dataPoints} data points (${pred.basedOn.method})`);
      console.log();
    });
    return;
  }

  if (cArgs.mode === "explain" && cArgs.question) {
    const viewResults = await viewMap({
      db,
      datumView: timingView,
      params: { limit: n || 50, include_docs: true, descending: true },
    });
    const documents = viewResults.rows.map(row => row.doc as DatumDocument).filter(Boolean);
    const explanation = await aiService.explainData(documents, cArgs.question);
    console.log("\n💡 " + explanation);
    return;
  }

  // Get input text from either input array or field (since fieldArgs captures it)
  const inputText = cArgs.input?.join(" ") || cArgs.field || "";
  if (!inputText && cArgs.mode === "parse") {
    throw new Error("Please provide input text or use --mode for insights/predictions");
  }

  const contextResults = await viewMap({
    db,
    datumView: timingView,
    params: { limit: 10, include_docs: true, descending: true },
  });
  const context = contextResults.rows.map(row => row.doc as DatumDocument).filter(Boolean);
  
  // Get primary interpretation and alternatives if confidence is low
  const primary = await aiService.parseNaturalLanguage(inputText, context);
  const interpretations = [primary];
  
  if (primary.confidence < 0.9) {
    const altPrompt = `Suggest 2-3 alternative interpretations for: "${inputText}"`;
    try {
      const altEntry = await aiService.parseNaturalLanguage(altPrompt, context);
      if (altEntry.value && typeof altEntry.value === "object" && "alternatives" in altEntry.value) {
        const alts = (altEntry.value as any).alternatives;
        interpretations.push(...alts);
      }
    } catch (error) {
      console.error("Failed to get alternative interpretations:", error);
    }
  }
  
  if (cArgs.interactive) {
    console.log("\n🤖 AI understood your input as:");
    interpretations.slice(0, 3).forEach((interp, i) => {
      console.log(`\n${i + 1}. Field: ${interp.field}`);
      console.log(`   Value: ${JSON.stringify(interp.value)}`);
      console.log(`   Time: ${interp.time?.utc || "now"}`);
      if (interp.duration) console.log(`   Duration: ${interp.duration} minutes`);
      console.log(`   Confidence: ${(interp.confidence * 100).toFixed(0)}%`);
    });

    const response = await prompts({
      type: "select",
      name: "choice",
      message: "Which interpretation is correct?",
      choices: [
        ...interpretations.slice(0, 3).map((_, i) => ({
          title: `Option ${i + 1}`,
          value: i,
        })),
        { title: "None - let me rephrase", value: -1 },
        { title: "Cancel", value: -2 },
      ],
    });

    if (response.choice === -2 || response.choice === undefined) {
      return;
    }

    if (response.choice === -1) {
      const newInput = await prompts({
        type: "text",
        name: "value",
        message: "Please rephrase your input:",
      });
      
      if (newInput.value) {
        return aiCmd([newInput.value], namespace);
      }
      return;
    }

    const selected = interpretations[response.choice];
    const payloadData = {
      ...(typeof selected.value === "object" ? selected.value : { value: selected.value }),
      field: selected.field,
      occurTime: selected.time || toDatumTime(DateTime.now()),
    };
    const payload = addIdAndMetadata(payloadData, { noMetadata: false });
    const doc = await addDoc({
      db,
      payload,
      outputArgs: cArgs,
    });

    console.log(`\n✅ Added: ${selected.field} = ${JSON.stringify(selected.value)}`);
    return;
  } else {
    const best = interpretations[0];
    const payloadData = {
      ...(typeof best.value === "object" ? best.value : { value: best.value }),
      field: best.field,
      occurTime: best.time || toDatumTime(DateTime.now()),
    };
    const payload = addIdAndMetadata(payloadData, { noMetadata: false });
    const doc = await addDoc({
      db,
      payload,
      outputArgs: cArgs,
    });

    console.log(`✅ Added: ${best.field} = ${JSON.stringify(best.value)}`);
    
    return;
  }
};

export const aiCmdBuilder = (yargs: any) => {
  return yargs
    .usage("$0 ai [input..]", "Use AI to parse natural language input or analyze data")
    .positional("input", {
      describe: "Natural language input to parse",
      type: "string",
      array: true,
    })
    .option("mode", {
      describe: "AI operation mode",
      choices: ["parse", "insights", "predict", "explain", "all"],
      default: "parse",
    })
    .option("api-key", {
      describe: "AI API key (ANTHROPIC_API_KEY for Claude, OPENAI_API_KEY for OpenAI)",
      type: "string",
    })
    .option("model", {
      describe: "AI model to use",
      type: "string",
      default: "claude-3-haiku-20240307",
    })
    .option("provider", {
      describe: "AI provider to use",
      choices: ["claude", "openai"],
      default: "claude",
    })
    .option("interactive", {
      describe: "Interactively confirm AI interpretation",
      type: "boolean",
      default: false,
      alias: "i",
    })
    .option("question", {
      describe: "Question to ask about your data (with --mode explain)",
      type: "string",
      alias: "q",
    })
    .option("n", {
      describe: "Number of documents to analyze",
      type: "number",
      default: 50,
    })
    .example("$0 ai went to gym for 45 minutes", "Parse natural language entry")
    .example("$0 ai --mode insights", "Generate insights from recent data")
    .example("$0 ai --mode predict --field weight,mood", "Predict future values")
    .example("$0 ai --mode explain -q 'When do I usually exercise?'", "Ask questions about data")
    .example("$0 ai ate pizza -i", "Interactive mode to confirm interpretation")
    .example("$0 ai --provider openai 'had coffee'", "Use OpenAI instead of Claude");
};