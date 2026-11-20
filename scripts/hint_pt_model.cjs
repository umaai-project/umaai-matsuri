#!/usr/bin/env node
'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');

const HINT_DISCOUNT = [0, 10, 20, 30, 35, 40];
const BASE_HINT_PROBABILITY = 0.075;
const SEED = 20261120;
const CSV_HEADER = 'skill_id,skill_name,rarity,group_id,group_rate,base_grade,base_pt';

function price(basePt, level) {
  return Math.floor(basePt * (100 - HINT_DISCOUNT[level]) / 100);
}

// Match the current f32 arithmetic and rounding, in the no-cut cohort.
function legacy(h, p) {
  const f = Math.fround;
  return Math.trunc(f(Math.floor(f(f(p) + f(f(h) * 6.5))) * f(1.88)));
}

function rateH(h, p, { r0, a, h0 }) {
  return r0 * p * (1 + a * h / (h + h0));
}

function scoreHP(h, p, { r0, k, lambda }) {
  if (h === 0 || p === 0) return r0 * p;
  const x = k * h;
  const y = lambda * p;
  return r0 * (p + x * y / (x + y));
}

function readWhiteCatalog(csv) {
  const [header, ...lines] = csv.trimEnd().split(/\r?\n/);
  assert.equal(header.replace(/^\uFEFF/, ''), CSV_HEADER, 'unexpected SO export header');
  const groups = new Set();
  const skills = [];
  for (const [i, line] of lines.entries()) {
    const match = /^(\d+),"(?:[^"\r\n]|"")*",(\d+),(-?\d+),(-?\d+),(-?\d+),(-?\d*)$/.exec(line);
    assert(match, `bad SO export row ${i + 2}`);
    const [, rarity, group, rate, grade, pt] = match.slice(1).map(Number);
    if (rarity !== 1 || rate !== 1 || group <= 0 || pt <= 0 || grade <= 0) continue;
    assert(!groups.has(group), `duplicate base white skill in group ${group}`);
    assert(price(pt, 5) > 0, `unexpected zero-cost skill in group ${group}`);
    groups.add(group);
    skills.push({ grade, pt });
  }
  return skills;
}

function rng(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

// Offline reference labels: independent base white skills, each purchasable once.
function scoreCurve(basket, levels, limit) {
  const best = new Int32Array(limit + 1);
  for (const [i, { grade, pt }] of basket.entries()) {
    const cost = price(pt, levels[i]);
    for (let budget = limit; budget >= cost; budget--) {
      best[budget] = Math.max(best[budget], best[budget - cost] + grade);
    }
  }
  return best;
}

function simulate(catalog, seed) {
  const random = rng(seed);
  const int = n => Math.floor(random() * n);
  const sizes = [64, 68, 72, 76, 80, 84];
  const shuffled = catalog.slice();
  assert(shuffled.length >= sizes.reduce((a, b) => a + b), 'need at least 444 base white skills');
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = int(i + 1);
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  const rows = [];
  let offset = 0;
  for (const [d, size] of sizes.entries()) {
    const basket = shuffled.slice(offset, offset + size);
    offset += size;
    const eligible = Math.floor(size * 0.6);
    const fullPrice = basket.reduce((sum, skill) => sum + skill.pt, 0);
    const opportunities = [0, 160, 400, 760, 1160, 1600];
    for (let r = 0; r < opportunities.length * 2; r++) {
      const levels = new Uint8Array(size);
      levels.fill(3, 0, 7);
      let h = 21;
      for (let i = 0; i < opportunities[r % opportunities.length]; i++) {
        if (random() >= BASE_HINT_PROBABILITY) continue;
        const target = int(eligible);
        const gain = [1, 3, 5][int(3)];
        // Nominal aggregate H can grow even when the selected skill cannot discount further.
        h += gain;
        levels[target] = Math.min(5, levels[target] + gain);
      }
      const budgets = [0.32, 0.49, 0.63, 0.75].map(fraction => Math.floor(fullPrice * fraction) + int(41) - 20);
      const ptDelta = [8, 16, 32][int(3)];
      const limit = Math.max(...budgets) + ptDelta;
      const before = scoreCurve(basket, levels, limit);
      const hintDelta = [1, 3, 5][int(3)];
      const targets = new Set();
      while (targets.size < 3) targets.add(int(eligible));
      const after = budgets.map(() => 0);
      for (const target of targets) {
        const old = levels[target];
        levels[target] = Math.min(5, old + hintDelta);
        const next = levels[target] === old ? before : scoreCurve(basket, levels, limit);
        for (const [i, p] of budgets.entries()) after[i] += next[p];
        levels[target] = old;
      }
      for (const [i, p] of budgets.entries()) {
        rows.push({
          deck: `basket-${d}`, run: `${d}/${r}/${i}`, h, p,
          nextH: h + hintDelta, ptDelta, score: before[p],
          hitScore: after[i] / targets.size, ptScore: before[p + ptDelta],
        });
      }
    }
  }
  return rows;
}

function validate(rows) {
  assert(Array.isArray(rows), 'expected a JSON array of labels');
  const runs = new Set();
  const decks = new Set();
  for (const row of rows) {
    const { run, deck, h, p, nextH, ptDelta, score, hitScore, ptScore } = row;
    assert(typeof run === 'string' && run && !runs.has(run), 'labels need unique run IDs');
    assert(typeof deck === 'string' && deck, 'labels need a deck ID');
    assert([h, p, nextH, ptDelta].every(n => Number.isSafeInteger(n) && n >= 0), `invalid H or PT: ${run}`);
    assert(nextH >= h && ptDelta > 0 && Number.isSafeInteger(p + ptDelta), `invalid paired delta: ${run}`);
    assert([score, hitScore, ptScore].every(n => Number.isFinite(n) && n >= 0), `invalid score: ${run}`);
    assert(hitScore >= score && ptScore >= score, `paired scores cannot decrease: ${run}`);
    if (p === 0) assert(score === 0 && hitScore === 0, `zero PT cannot buy skills: ${run}`);
    if (nextH === h) assert(hitScore === score, `no Hint change requires no score change: ${run}`);
    runs.add(run);
    decks.add(deck);
  }
  assert(decks.size >= 3, 'need at least three decks or disjoint baskets');
  return [...decks];
}

function values(score, row) {
  const base = score(row.h, row.p);
  return [base, score(row.nextH, row.p) - base, score(row.h, row.p + row.ptDelta) - base];
}

function observed(row) {
  return [row.score, row.hitScore - row.score, row.ptScore - row.score];
}

function fit(train) {
  // Normalize on training rows so the total score does not drown out the two margins.
  const targets = train.map(observed);
  const weights = [0, 1, 2].map(i => 1 / Math.max(1, targets.reduce((s, y) => s + y[i] ** 2, 0) / targets.length));
  function choose(formula, grid) {
    let best;
    for (const shape of grid) {
      const unit = (h, p) => formula(h, p, { ...shape, r0: 1 });
      let xx = 0;
      let xy = 0;
      let yy = 0;
      for (const [j, row] of train.entries()) {
        values(unit, row).forEach((x, i) => {
          const y = targets[j][i];
          xx += weights[i] * x * x;
          xy += weights[i] * x * y;
          yy += weights[i] * y * y;
        });
      }
      const r0 = xx > 0 ? Math.max(0, xy / xx) : 0;
      const params = { r0, ...shape };
      const loss = yy - 2 * r0 * xy + r0 * r0 * xx;
      if (!best || loss < best.loss) best = { params, score: (h, p) => formula(h, p, params), loss };
    }
    return best;
  }
  const bounds = [0.1, 0.2, 0.35, 0.5, 2 / 3];
  return {
    legacy: { score: legacy },
    fixed188: { score: (_h, p) => 1.88 * p },
    ptOnly: choose((_h, p, { r0 }) => r0 * p, [{}]),
    rateH: choose(rateH, bounds.flatMap(a => [15, 40, 80, 160, 320].map(h0 => ({ a, h0 })))),
    scoreHP: choose(scoreHP, bounds.flatMap(lambda => [0.75, 1.5, 3, 4.5, 6.5, 9, 12, 16].map(k => ({ k, lambda })))),
  };
}

function evaluate(rows) {
  const decks = validate(rows);
  const folds = [];
  const totals = {};
  for (const deck of decks) {
    const train = rows.filter(row => row.deck !== deck);
    const test = rows.filter(row => row.deck === deck);
    const metrics = {};
    for (const [name, { score, params }] of Object.entries(fit(train))) {
      const sums = { scoreMAE: 0, hintMAE: 0, ptMAE: 0, regret: 0 };
      const total = totals[name] ??= { ...sums };
      for (const row of test) {
        const [base, hint, pt] = values(score, row);
        const [baseTrue, hintTrue, ptTrue] = observed(row);
        sums.scoreMAE += Math.abs(base - baseTrue);
        sums.hintMAE += Math.abs(hint - hintTrue);
        sums.ptMAE += Math.abs(pt - ptTrue);
        sums.regret += Math.max(hintTrue, ptTrue) - (hint > pt ? hintTrue : ptTrue);
      }
      for (const key of Object.keys(sums)) total[key] += sums[key];
      metrics[name] = { ...mean(sums, test.length), params };
    }
    folds.push({ deck, rows: test.length, metrics });
  }
  const allSampleFit = Object.fromEntries(Object.entries(fit(rows)).filter(([, model]) => model.params).map(([name, model]) => [name, model.params]));
  return {
    rows: rows.length, decks: decks.length,
    hRange: range(rows.map(row => row.h)), pRange: range(rows.map(row => row.p)),
    holdout: Object.fromEntries(Object.entries(totals).map(([name, sums]) => [name, mean(sums, rows.length)])),
    folds, allSampleFit,
  };
}

function range(ns) {
  return ns.reduce(([lo, hi], n) => [Math.min(lo, n), Math.max(hi, n)], [Infinity, -Infinity]);
}

function mean(sums, count) {
  return Object.fromEntries(Object.entries(sums).map(([key, sum]) => [key, Number((sum / count).toFixed(3))]));
}

function test() {
  assert.equal(legacy(383, 9037), 21668);
  assert.equal(legacy(383, 0), 4679);
  assert.deepEqual(HINT_DISCOUNT.map((_, h) => price(113, h)), [113, 101, 90, 79, 73, 67]);
  const csv = `${CSV_HEADER}\n1,"a, ""b""",1,10,1,217,200\n2,"gold",2,10,1,508,200\n3,"upgrade",1,10,2,288,180\n4,"missing",1,-1,1,217,`;
  const catalog = readWhiteCatalog(csv);
  assert.deepEqual(catalog, [{ grade: 217, pt: 200 }]);
  assert.equal(scoreCurve(catalog, [5], 240)[240], 217);
  const basket = [...catalog, { grade: 263, pt: 200 }];
  assert.equal(scoreCurve(basket, [3, 3], 285)[285], 480);
  assert.equal(scoreCurve(basket, [5, 1], 285)[285], 263);
  assert.equal(scoreCurve(basket, [5, 1], 300)[300], 480);
  const r0 = 1.4;
  for (const score of [(h, p) => rateH(h, p, { r0, a: 0.35, h0: 80 }), (h, p) => scoreHP(h, p, { r0, k: 6.5, lambda: 0.4 })]) {
    const tol = 1e-9;
    for (const p of [0, 100, 1000, 9037, 13000]) {
      assert.equal(score(0, p), r0 * p);
      for (let h = 0; h < 500; h++) {
        const base = score(h, p);
        if (p === 0) assert.equal(base, 0);
        assert(score(h, p + 1) >= base);
        assert(score(h + 1, p) >= base);
        assert(score(h + 2, p) - score(h + 1, p) <= score(h + 1, p) - base + tol);
        assert(base <= r0 * p * (1 + 0.4) + tol);
      }
    }
  }
  const rows = Array.from({ length: 3 }, (_, i) => ({
    run: `r${i}`, deck: `d${i}`, h: 21, p: 100 + i, nextH: 24, ptDelta: 10,
    score: 2 * (100 + i), hitScore: 2 * (100 + i), ptScore: 2 * (110 + i),
  }));
  assert.equal(evaluate(rows).holdout.ptOnly.scoreMAE, 0);
  assert.equal(evaluate(rows).holdout.ptOnly.ptMAE, 0);
  assert.throws(() => evaluate([rows[0], rows[0], rows[1], rows[2]]), /unique run/);
}

if (require.main === module) {
  const [mode, path, seedArg, ...extra] = process.argv.slice(2);
  const usage = 'Usage: node scripts/hint_pt_model.cjs --test | --csv SO-export.csv [seed] | --labels paired-labels.json';
  assert(!extra.length, usage);
  if (mode === '--test' && path === undefined) {
    test();
    console.log('OK: prices, legacy rounding, budget labels, model bounds and deck holdout');
  } else if (mode === '--csv' && path) {
    const seed = seedArg === undefined ? SEED : Number(seedArg);
    assert((seedArg === undefined || /^\d+$/.test(seedArg)) && Number.isInteger(seed) && seed >= 0 && seed <= 0xffffffff, 'seed must be a uint32');
    const catalog = readWhiteCatalog(fs.readFileSync(path, 'utf8'));
    console.log(JSON.stringify({
      source: 'SO base price/grade; synthetic availability, H and PT', seed,
      baseHintProbability: BASE_HINT_PROBABILITY, catalogSkills: catalog.length,
      ...evaluate(simulate(catalog, seed)),
    }, null, 2));
  } else if (mode === '--labels' && path && seedArg === undefined) {
    console.log(JSON.stringify(evaluate(JSON.parse(fs.readFileSync(path, 'utf8'))), null, 2));
  } else {
    console.error(usage);
    process.exitCode = mode === '--help' && !path ? 0 : 2;
  }
}

module.exports = { legacy, rateH, scoreHP };
