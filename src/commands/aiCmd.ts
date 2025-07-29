import { ArgumentParser } from "argparse";
import { DatumDocument } from "../documentControl/DatumDocument";
import { lastDocs } from "../documentControl/lastDocs";
import { addDoc } from "../documentControl/addDoc";
import { AIService } from "../ai/aiService";
import { parseNaturalLanguage, suggestInterpretations } from "../ai/nlpParser";
import { occurredFields } from "../field/occurredFields";
import { output } from "../output/output";
import prompts from "prompts";
import { datumTime } from "../time/datumTime";
import { MainDatumArgs } from "../input/mainArgs";
import { mergeConfigIntoArgs } from "../config/mergeConfigIntoArgs";
import { fieldArgs } from "../input/fieldArgs";
import { flexiblePositional } from "../input/flexiblePositional";
import { outputArgs } from "../input/outputArgs";

export const aiCmd = async (
  args: string[],
  namespace: MainDatumArgs,
): Promise<DatumDocument | DatumDocument[] | void> => {
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

  parser.add_argument("--api-key", {
    help: "OpenAI API key (or set OPENAI_API_KEY env var)",
  });

  parser.add_argument("--model", {
    help: "OpenAI model to use",
    default: "gpt-4o-mini",
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

  const cmdArgs = flexiblePositional(
    args,
    parser,
    namespace,
    ["mode", "model", "api-key", "interactive", "question", "n"],
    "input",
  );

  const cArgs = await mergeConfigIntoArgs(cmdArgs);
  const { db, n } = cArgs;
  
  const apiKey = process.env.OPENAI_API_KEY || cArgs.api_key;
  if (!apiKey) {
    throw new Error("OpenAI API key required. Set OPENAI_API_KEY environment variable or pass --api-key");
  }

  const aiService = new AIService({
    apiKey,
    model: cArgs.model || "gpt-4o-mini",
    enableNLP: cArgs.mode !== "insights" && cArgs.mode !== "predict",
    enableInsights: cArgs.mode === "insights" || cArgs.mode === "all",
    enablePredictions: cArgs.mode === "predict" || cArgs.mode === "all",
  });

  if (cArgs.mode === "insights") {
    const documents = await lastDocs({ db, n: n || 100 });
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
    const documents = await lastDocs({ db, n: n || 100 });
    const predictions = await aiService.generatePredictions(documents, fields);

    if (predictions.length === 0) {
      console.log("No predictions generated. Need more historical data.");
      return;
    }

    console.log("\n🔮 AI Predictions:\n");
    predictions.forEach((pred) => {
      console.log(`${pred.field}: ${JSON.stringify(pred.predictedValue)}`);
      console.log(`   Date: ${pred.date.human}`);
      console.log(`   Confidence: ${(pred.confidence * 100).toFixed(0)}%`);
      console.log(`   Based on: ${pred.basedOn.dataPoints} data points (${pred.basedOn.method})`);
      console.log();
    });
    return;
  }

  if (cArgs.mode === "explain" && cArgs.question) {
    const documents = await lastDocs({ db, n: n || 50 });
    const explanation = await aiService.explainData(documents, cArgs.question);
    console.log("\n💡 " + explanation);
    return;
  }

  const inputText = cArgs.input?.join(" ") || "";
  if (!inputText) {
    throw new Error("Please provide input text or use --mode for insights/predictions");
  }

  const context = await lastDocs({ db, n: 10 });
  const interpretations = await suggestInterpretations(inputText, aiService, context);
  
  if (cArgs.interactive) {
    console.log("\n🤖 AI understood your input as:");
    interpretations.slice(0, 3).forEach((interp, i) => {
      console.log(`\n${i + 1}. Field: ${interp.field}`);
      console.log(`   Value: ${JSON.stringify(interp.value)}`);
      console.log(`   Time: ${interp.time?.human || "now"}`);
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
    const doc = await addDoc({
      db,
      data: selected.value,
      field: selected.field,
      occurTime: selected.time || datumTime(),
      duration: selected.duration,
      meta: { aiParsed: true, confidence: selected.confidence },
    });

    console.log(`\n✅ Added: ${selected.field} = ${JSON.stringify(selected.value)}`);
    return doc;
  } else {
    const best = interpretations[0];
    const doc = await addDoc({
      db,
      data: best.value,
      field: best.field,
      occurTime: best.time || datumTime(),
      duration: best.duration,
      meta: { aiParsed: true, confidence: best.confidence },
    });

    await output({ 
      args: { ...cArgs, view: "ai", format: cArgs.format || "json" }, 
      data: doc,
    });
    
    return doc;
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
      describe: "OpenAI API key (or set OPENAI_API_KEY env var)",
      type: "string",
    })
    .option("model", {
      describe: "OpenAI model to use",
      type: "string",
      default: "gpt-4o-mini",
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
    .example("$0 ai ate pizza -i", "Interactive mode to confirm interpretation");
};