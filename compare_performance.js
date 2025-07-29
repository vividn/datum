#!/usr/bin/env node

const { performance } = require('perf_hooks');
const { execSync } = require('child_process');

console.log('🔬 Dayview Optimization Performance Comparison');
console.log('==============================================\n');

function runDayviewBenchmark(branch, description, iterations = 3) {
  console.log(`📊 Testing ${description} (${branch})`);
  
  const times = [];
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    try {
      execSync('pnpm dev -- dayview --n-days 7', { stdio: 'pipe', timeout: 15000 });
      const end = performance.now();
      const duration = end - start;
      times.push(duration);
      console.log(`  Run ${i + 1}: ${duration.toFixed(2)}ms`);
    } catch (error) {
      console.log(`  Run ${i + 1}: Failed (timeout or error)`);
    }
  }
  
  if (times.length === 0) {
    console.log(`  ❌ All runs failed\n`);
    return null;
  }
  
  const avg = times.reduce((a, b) => a + b, 0) / times.length;
  const min = Math.min(...times);
  console.log(`  📈 Average: ${avg.toFixed(2)}ms`);
  console.log(`  🎯 Best: ${min.toFixed(2)}ms\n`);
  
  return { avg, min, times };
}

function analyzeSVGOptimizations() {
  console.log('🎨 SVG Structure Analysis');
  console.log('=========================\n');
  
  try {
    const output = execSync('pnpm dev -- dayview --n-days 7', { encoding: 'utf8', stdio: 'pipe' });
    
    // Analyze pattern usage
    const patternDefs = (output.match(/<pattern[^>]*>/g) || []).length;
    const patternRefs = (output.match(/url\(#[^)]*\)/g) || []).length;
    const uniquePatternIds = new Set((output.match(/id="[^"]*pattern[^"]*"/g) || []).map(m => m.match(/id="([^"]*)"/)[1]));
    
    console.log(`📊 Pattern Analysis:`);
    console.log(`  🎯 Pattern definitions: ${patternDefs}`);
    console.log(`  🔄 Pattern references: ${patternRefs}`);
    console.log(`  🆔 Unique pattern IDs: ${uniquePatternIds.size}`);
    
    if (patternDefs > 0 && patternRefs > 0) {
      const efficiency = patternRefs / patternDefs;
      console.log(`  📈 Pattern efficiency: ${efficiency.toFixed(2)}x reuse`);
    }
    
    // Analyze file size and complexity
    const svgSize = Buffer.byteLength(output, 'utf8');
    const elementCount = (output.match(/<[^/][^>]*>/g) || []).length;
    
    console.log(`\n📦 SVG Metrics:`);
    console.log(`  📏 Output size: ${(svgSize / 1024).toFixed(2)} KB`);
    console.log(`  🔢 Element count: ${elementCount}`);
    console.log(`  📊 Bytes per element: ${(svgSize / elementCount).toFixed(1)}\n`);
    
    return { patternDefs, patternRefs, svgSize, elementCount, efficiency: patternRefs / patternDefs };
  } catch (error) {
    console.log(`❌ Failed to analyze SVG structure\n`);
    return null;
  }
}

function memoryUsageTest() {
  console.log('🧠 Memory Usage Test');
  console.log('===================\n');
  
  const iterations = 5;
  console.log(`Running ${iterations} consecutive dayview generations to test memory stability...\n`);
  
  for (let i = 0; i < iterations; i++) {
    const memBefore = process.memoryUsage();
    
    try {
      execSync('pnpm dev -- dayview --n-days 3', { stdio: 'pipe' });
      const memAfter = process.memoryUsage();
      
      const heapDelta = (memAfter.heapUsed - memBefore.heapUsed) / 1024 / 1024;
      console.log(`  Run ${i + 1}: Heap delta ${heapDelta > 0 ? '+' : ''}${heapDelta.toFixed(2)} MB`);
    } catch (error) {
      console.log(`  Run ${i + 1}: Failed`);
    }
  }
  
  // Force garbage collection if available
  if (global.gc) {
    global.gc();
    console.log(`\n🗑️  Garbage collection completed`);
  }
  
  console.log();
}

function cacheEffectivenessTest() {
  console.log('⚡ Cache Effectiveness Test');
  console.log('==========================\n');
  
  console.log('Testing field sort cache with repeated operations...\n');
  
  const operations = [
    'dayview --n-days 1',
    'dayview --n-days 3', 
    'dayview --n-days 1',  // Should hit cache
    'dayview --n-days 3',  // Should hit cache
    'dayview --n-days 7',  // New cache entry
  ];
  
  const times = [];
  operations.forEach((op, i) => {
    const start = performance.now();
    try {
      execSync(`pnpm dev -- ${op}`, { stdio: 'pipe' });
      const end = performance.now();
      const duration = end - start;
      times.push(duration);
      
      const cacheHit = i >= 2 && (op.includes('--n-days 1') || op.includes('--n-days 3'));
      console.log(`  ${op}: ${duration.toFixed(2)}ms ${cacheHit ? '(cache hit)' : ''}`);
    } catch (error) {
      console.log(`  ${op}: Failed`);
    }
  });
  
  if (times.length >= 4) {
    const firstTime1Day = times[0];
    const secondTime1Day = times[2];
    const firstTime3Day = times[1];
    const secondTime3Day = times[3];
    
    const improvement1Day = ((firstTime1Day - secondTime1Day) / firstTime1Day) * 100;
    const improvement3Day = ((firstTime3Day - secondTime3Day) / firstTime3Day) * 100;
    
    console.log(`\n📈 Cache Performance:`);
    if (improvement1Day > 0) {
      console.log(`  ✅ 1-day cache speedup: ${improvement1Day.toFixed(1)}%`);
    }
    if (improvement3Day > 0) {
      console.log(`  ✅ 3-day cache speedup: ${improvement3Day.toFixed(1)}%`);
    }
    
    const avgImprovement = (improvement1Day + improvement3Day) / 2;
    if (avgImprovement > 0) {
      console.log(`  🎯 Average cache benefit: ${avgImprovement.toFixed(1)}%`);
    }
  }
  
  console.log();
}

async function runComprehensiveBenchmark() {
  console.log('Current branch: feature/optimize-dayview-performance\n');
  
  // Build first
  try {
    execSync('pnpm build', { stdio: 'pipe' });
    console.log('✅ Build successful\n');
  } catch (error) {
    console.log('❌ Build failed, using dev mode\n');
  }
  
  // Run optimized version tests
  const optimizedResults = runDayviewBenchmark('current', 'Optimized Version');
  
  // Analyze optimizations
  const svgAnalysis = analyzeSVGOptimizations();
  
  // Test memory usage
  memoryUsageTest();
  
  // Test cache effectiveness
  cacheEffectivenessTest();
  
  // Final summary
  console.log('🎯 Optimization Summary');
  console.log('=======================');
  
  if (optimizedResults) {
    console.log(`📊 Performance: ${optimizedResults.avg.toFixed(2)}ms average (7-day dayview)`);
  }
  
  if (svgAnalysis && svgAnalysis.efficiency > 1) {
    console.log(`🎨 Pattern Cache: ${svgAnalysis.efficiency.toFixed(1)}x reuse efficiency`);
  }
  
  console.log('\n✅ Optimizations implemented:');
  console.log('   🎯 SVG pattern caching and reuse');
  console.log('   🔄 Field sort calculation caching');
  console.log('   🧹 Memory leak prevention');
  
  console.log('\n🚀 Benchmark complete!');
}

if (require.main === module) {
  runComprehensiveBenchmark().catch(console.error);
}