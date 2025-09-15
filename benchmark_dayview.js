#!/usr/bin/env node

const { performance } = require('perf_hooks');
const { execSync } = require('child_process');
const fs = require('fs');

console.log('🚀 Dayview Performance Benchmark');
console.log('================================\n');

// Benchmark configurations
const configs = [
  { name: 'Single Day', args: '--n-days 1' },
  { name: '3 Days', args: '--n-days 3' },
  { name: '7 Days (1 week)', args: '--n-days 7' },
  { name: '14 Days (2 weeks)', args: '--n-days 14' },
];

// Function to run benchmark
function runBenchmark(name, command, iterations = 5) {
  console.log(`📊 Benchmarking: ${name}`);
  const times = [];
  
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    try {
      execSync(command, { stdio: 'pipe', timeout: 30000 });
    } catch (error) {
      console.warn(`  ⚠️  Run ${i + 1} failed, skipping...`);
      continue;
    }
    const end = performance.now();
    const duration = end - start;
    times.push(duration);
    console.log(`  Run ${i + 1}: ${duration.toFixed(2)}ms`);
  }
  
  if (times.length === 0) {
    console.log(`  ❌ All runs failed for ${name}\n`);
    return null;
  }
  
  const avg = times.reduce((a, b) => a + b, 0) / times.length;
  const min = Math.min(...times);
  const max = Math.max(...times);
  const stdDev = Math.sqrt(times.reduce((a, b) => a + Math.pow(b - avg, 2), 0) / times.length);
  
  console.log(`  📈 Average: ${avg.toFixed(2)}ms`);
  console.log(`  🎯 Min: ${min.toFixed(2)}ms, Max: ${max.toFixed(2)}ms`);
  console.log(`  📊 Std Dev: ${stdDev.toFixed(2)}ms\n`);
  
  return { avg, min, max, stdDev, times };
}

// Function to measure pattern cache effectiveness
function measurePatternCache() {
  console.log('🎨 Pattern Cache Effectiveness');
  console.log('==============================\n');
  
  // Generate a dayview with multi-state data
  const command = 'pnpm dev -- dayview --n-days 7';
  console.log('📊 Analyzing pattern usage in 7-day dayview...');
  
  try {
    const output = execSync(command, { encoding: 'utf8', stdio: 'pipe' });
    
    // Count pattern definitions vs usage
    const patternDefs = (output.match(/<pattern[^>]*id="[^"]*"/g) || []).length;
    const patternUsages = (output.match(/fill="url\(#[^)]*\)"/g) || []).length;
    
    console.log(`  🎯 Pattern definitions: ${patternDefs}`);
    console.log(`  🔄 Pattern usages: ${patternUsages}`);
    
    if (patternDefs > 0) {
      const reuseRatio = patternUsages / patternDefs;
      console.log(`  📈 Pattern reuse ratio: ${reuseRatio.toFixed(2)}x`);
      
      if (reuseRatio > 1) {
        console.log(`  ✅ Patterns are being reused effectively!`);
      } else {
        console.log(`  ⚠️  Low pattern reuse - may need optimization`);
      }
    }
    
    // Measure SVG size
    const svgSize = Buffer.byteLength(output, 'utf8');
    console.log(`  📦 SVG output size: ${(svgSize / 1024).toFixed(2)} KB\n`);
    
    return { patternDefs, patternUsages, reuseRatio: patternUsages / patternDefs, svgSize };
  } catch (error) {
    console.log(`  ❌ Failed to analyze pattern usage\n`);
    return null;
  }
}

// Function to test field sort caching
function measureFieldSortCache() {
  console.log('🔄 Field Sort Cache Performance');
  console.log('===============================\n');
  
  const iterations = 3;
  const command = 'pnpm dev -- dayview --n-days 3';
  
  console.log(`📊 Running ${iterations} consecutive dayview generations...`);
  console.log('   (Second+ runs should benefit from field sort caching)\n');
  
  const times = [];
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    try {
      execSync(command, { stdio: 'pipe' });
      const end = performance.now();
      const duration = end - start;
      times.push(duration);
      console.log(`  Run ${i + 1}: ${duration.toFixed(2)}ms`);
    } catch (error) {
      console.log(`  Run ${i + 1}: Failed`);
    }
  }
  
  if (times.length >= 2) {
    const firstRun = times[0];
    const avgSubsequent = times.slice(1).reduce((a, b) => a + b, 0) / (times.length - 1);
    const improvement = ((firstRun - avgSubsequent) / firstRun) * 100;
    
    console.log(`\n  📈 First run: ${firstRun.toFixed(2)}ms`);
    console.log(`  🔄 Avg subsequent: ${avgSubsequent.toFixed(2)}ms`);
    
    if (improvement > 0) {
      console.log(`  ✅ Cache speedup: ${improvement.toFixed(1)}%`);
    } else {
      console.log(`  ⚠️  No significant cache benefit detected`);
    }
  }
  
  return times;
}

// Run all benchmarks
async function runAllBenchmarks() {
  console.log('Building project first...\n');
  try {
    execSync('pnpm build', { stdio: 'pipe' });
    console.log('✅ Build successful\n');
  } catch (error) {
    console.log('❌ Build failed, using dev mode\n');
  }
  
  const results = {};
  
  // Core performance benchmarks
  for (const config of configs) {
    const command = `pnpm dev -- dayview ${config.args}`;
    results[config.name] = runBenchmark(config.name, command);
  }
  
  // Pattern cache analysis
  results.patternCache = measurePatternCache();
  
  // Field sort cache test
  results.fieldSortCache = measureFieldSortCache();
  
  // Summary
  console.log('\n🎯 Performance Summary');
  console.log('======================');
  
  // Show performance scaling
  const validResults = Object.entries(results).filter(([k, v]) => v && v.avg);
  if (validResults.length >= 2) {
    console.log('\n📈 Performance scaling:');
    validResults.forEach(([name, data]) => {
      if (data.avg) {
        console.log(`  ${name}: ${data.avg.toFixed(2)}ms average`);
      }
    });
  }
  
  // Show optimization benefits
  if (results.patternCache && results.patternCache.reuseRatio > 1) {
    console.log(`\n✅ Pattern optimization: ${results.patternCache.reuseRatio.toFixed(1)}x reuse ratio`);
  }
  
  console.log('\n🚀 Benchmark complete!');
}

// Run if called directly
if (require.main === module) {
  runAllBenchmarks().catch(console.error);
}

module.exports = { runBenchmark, measurePatternCache, measureFieldSortCache };