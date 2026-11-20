'use strict';

const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const cli = path.join(__dirname, 'hint_pt_model.cjs');
const header = 'skill_id,skill_name,rarity,group_id,group_rate,base_grade,base_pt';
const models = ['legacy', 'fixed188', 'ptOnly', 'rateH', 'scoreHP'];

function run(...args) {
  const result = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', timeout: 20_000 });
  assert.ifError(result.error);
  return result;
}

function report(...args) {
  const result = run(...args);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

function fixture(t, name, contents) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hint-pt-test-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const file = path.join(dir, name);
  fs.writeFileSync(file, contents);
  return file;
}

function syntheticCsv() {
  const white = Array.from({ length: 444 }, (_, i) => {
    const grade = 110 + (i * 37 % 211);
    const pt = 109 + (i * 53 % 119);
    return `${i + 1},"white, ""${i + 1}""",1,${i + 1},1,${grade},${pt}`;
  });
  return `\uFEFF${[header, ...white, '445,"gold",2,1,1,508,200', '446,"upgrade",1,1,2,300,180', '447,"missing cost",1,-1,1,200,'].join('\r\n')}\r\n`;
}

function checkMetrics(metrics, count) {
  assert.deepEqual(Object.keys(metrics), models);
  for (const model of models) {
    const { scoreMAE, hintMAE, ptMAE, regret } = metrics[model];
    assert([scoreMAE, hintMAE, ptMAE, regret].every(x => Number.isFinite(x) && x >= 0), model);
  }
  assert.equal(count > 0, true);
}

test('unit tests cover prices, f32 baseline, budget frontiers and model bounds', () => {
  const result = run('--test');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^OK:/);
});

test('CSV mode is deterministic and evaluates each entire pool on three seeds', t => {
  const csv = fixture(t, 'synthetic.csv', syntheticCsv());
  const reference = report('--csv', csv);
  assert.deepEqual(reference, report('--csv', csv, '20261120'));
  for (const [seed, result] of [
    [20261120, reference],
    [20261121, report('--csv', csv, '20261121')],
    [20261122, report('--csv', csv, '20261122')],
  ]) {
    assert.equal(result.seed, seed);
    assert.equal(result.baseHintProbability, 0.075);
    assert.equal(result.catalogSkills, 444);
    assert.equal(result.hRange[0], 21);
    assert(result.hRange[1] > 21);
    assert.equal(result.rows, 288);
    assert.equal(result.decks, 6);
    assert.deepEqual(result.folds.map(fold => [fold.deck, fold.rows]),
      Array.from({ length: 6 }, (_, i) => [`basket-${i}`, 48]));
    checkMetrics(result.holdout, result.rows);
    for (const fold of result.folds) checkMetrics(fold.metrics, fold.rows);
  }
  for (const seed of ['-1', '4294967296', '1.5']) {
    const failed = run('--csv', csv, seed);
    assert.notEqual(failed.status, 0);
    assert.match(failed.stderr, /seed must be a uint32/);
  }
  const malformed = fixture(t, 'malformed.csv', `${header}\nnot a skill\n`);
  const failed = run('--csv', malformed);
  assert.notEqual(failed.status, 0);
  assert.match(failed.stderr, /bad SO export row 2/);
});

function labels() {
  return ['a', 'b', 'c'].flatMap((deck, i) => [100, 150].map((p, j) => ({
    run: `${deck}-${j}`, deck, h: 21 + 37 * i, p, nextH: 24 + 37 * i, ptDelta: 10,
    score: 2 * p, hitScore: 2 * p, ptScore: 2 * (p + 10),
  })));
}

test('paired labels hold out every row in the evaluation deck', t => {
  const rows = labels();
  const original = report('--labels', fixture(t, 'labels.json', JSON.stringify(rows)));
  assert.equal(original.rows, 6);
  assert.equal(original.decks, 3);
  assert.deepEqual(original.holdout.ptOnly, { scoreMAE: 0, hintMAE: 0, ptMAE: 0, regret: 0 });

  const changed = rows.map(row => row.deck === 'a' ? {
    ...row, score: row.score + 100, hitScore: row.hitScore + 125, ptScore: row.ptScore + 100,
  } : row);
  const outlier = report('--labels', fixture(t, 'outlier.json', JSON.stringify(changed)));
  const [before] = original.folds;
  const [after] = outlier.folds;
  assert.equal(after.deck, 'a');
  for (const model of ['ptOnly', 'rateH', 'scoreHP']) {
    assert.deepEqual(after.metrics[model].params, before.metrics[model].params);
  }
  assert.equal(after.metrics.ptOnly.scoreMAE, 100);
  assert.equal(after.metrics.ptOnly.hintMAE, 25);
  assert.equal(after.metrics.ptOnly.regret, 5);
  assert.notDeepEqual(outlier.allSampleFit.ptOnly, original.allSampleFit.ptOnly);
});

test('the fitted H/P formula recovers known parameters on held-out labels', t => {
  const known = (h, p) => 1.35 * (p + 4.5 * h * 0.35 * p / (4.5 * h + 0.35 * p));
  const rows = Array.from({ length: 4 }, (_, deck) =>
    [900, 2400, 6100, 9200].map((p, i) => {
      const h = [21, 72, 163, 310][(deck + i) % 4] + deck;
      const nextH = h + [1, 3, 5, 3][i];
      const ptDelta = [8, 16, 32, 16][i];
      return {
        run: `${deck}/${i}`, deck: `known-${deck}`, h, p, nextH, ptDelta,
        score: known(h, p), hitScore: known(nextH, p), ptScore: known(h, p + ptDelta),
      };
    })).flat();
  const result = report('--labels', fixture(t, 'known.json', JSON.stringify(rows)));
  assert.deepEqual(result.holdout.scoreHP, { scoreMAE: 0, hintMAE: 0, ptMAE: 0, regret: 0 });
  for (const { metrics } of result.folds) {
    const { r0, k, lambda } = metrics.scoreHP.params;
    assert(Math.abs(r0 - 1.35) < 1e-10);
    assert.equal(k, 4.5);
    assert.equal(lambda, 0.35);
  }
});

test('label mode rejects invalid or unpaired rows', t => {
  const rows = labels();
  const cases = [
    [[rows[0], ...rows], /unique run/],
    [rows.slice(0, 4), /at least three decks/],
    [[{ ...rows[0], h: 1.5 }, ...rows.slice(1)], /invalid H or PT/],
    [[{ ...rows[0], nextH: 0 }, ...rows.slice(1)], /invalid paired delta/],
    [[{ ...rows[0], ptScore: 1 }, ...rows.slice(1)], /paired scores cannot decrease/],
    [[{ ...rows[0], p: 0 }, ...rows.slice(1)], /zero PT cannot buy skills/],
    [[{ ...rows[0], nextH: rows[0].h, hitScore: rows[0].score + 1 }, ...rows.slice(1)], /no Hint change/],
  ];
  for (const [invalid, message] of cases) {
    const result = run('--labels', fixture(t, 'invalid.json', JSON.stringify(invalid)));
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, message);
  }
});
