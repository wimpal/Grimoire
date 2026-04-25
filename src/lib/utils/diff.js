/** @typedef {{ type: 'add' | 'remove' | 'unchanged', lines: string[] }} DiffHunk */

/**
 * Compute a line-level diff between two texts.
 * Returns an array of hunks, each representing a contiguous block of
 * added, removed, or unchanged lines.
 *
 * @param {string} original
 * @param {string} improved
 * @returns {DiffHunk[]}
 */
export function computeDiff(original, improved) {
  const a = original.split('\n');
  const b = improved.split('\n');
  return diffLines(a, b);
}

/**
 * Apply accepted hunks to the original text, producing a partially-improved result.
 * `accepted` is a Set of hunk indices.
 *
 * @param {DiffHunk[]} hunks
 * @param {Set<number>} acceptedIndices
 * @param {string} original
 * @param {string} improved
 * @returns {string}
 */
export function applyAcceptedHunks(hunks, acceptedIndices, original, improved) {
  const a = original.split('\n');
  const b = improved.split('\n');
  let ai = 0;
  const result = [];

  for (let hi = 0; hi < hunks.length; hi++) {
    const hunk = hunks[hi];
    if (hunk.type === 'unchanged') {
      result.push(...hunk.lines);
      ai += hunk.lines.length;
    } else if (hunk.type === 'remove') {
      if (acceptedIndices.has(hi)) {
        ai += hunk.lines.length;
      } else {
        result.push(...hunk.lines);
        ai += hunk.lines.length;
      }
    } else if (hunk.type === 'add') {
      if (acceptedIndices.has(hi)) {
        result.push(...hunk.lines);
      }
    }
  }

  return result.join('\n');
}

// ── Internal: LCS-based line diff ──────────────────────────────────────────

/**
 * Myers-like diff using LCS on lines.
 * Returns groups of (added/removed/unchanged) line blocks.
 *
 * @param {string[]} a original lines
 * @param {string[]} b improved lines
 * @returns {DiffHunk[]}
 */
function diffLines(a, b) {
  const lcs = computeLCS(a, b);
  const hunks = [];
  let ai = 0, bi = 0, li = 0;

  while (ai < a.length || bi < b.length) {
    if (li < lcs.length) {
      const lcsLine = lcs[li];
      // Find where this LCS line appears next in both arrays.
      let nextA = ai;
      while (nextA < a.length && a[nextA] !== lcsLine) nextA++;
      let nextB = bi;
      while (nextB < b.length && b[nextB] !== lcsLine) nextB++;

      // Emit removed lines (in a but not in b) before the next common line.
      if (nextA > ai) {
        hunks.push({ type: 'remove', lines: a.slice(ai, nextA) });
      }
      // Emit added lines (in b but not in a) before the next common line.
      if (nextB > bi) {
        hunks.push({ type: 'add', lines: b.slice(bi, nextB) });
      }

      // Emit the common line.
      hunks.push({ type: 'unchanged', lines: [lcsLine] });

      ai = nextA + 1;
      bi = nextB + 1;
      li++;
    } else {
      // No more LCS — remaining lines are removes and/or adds.
      if (ai < a.length) {
        hunks.push({ type: 'remove', lines: a.slice(ai) });
      }
      if (bi < b.length) {
        hunks.push({ type: 'add', lines: b.slice(bi) });
      }
      break;
    }
  }

  return mergeAdjacentHunks(hunks);
}

/**
 * Merge adjacent hunks of the same type for cleaner display.
 *
 * @param {DiffHunk[]} hunks
 * @returns {DiffHunk[]}
 */
function mergeAdjacentHunks(hunks) {
  const merged = [];
  for (const h of hunks) {
    const last = merged[merged.length - 1];
    if (last && last.type === h.type) {
      last.lines.push(...h.lines);
    } else {
      merged.push(h);
    }
  }
  return merged;
}

/**
 * Compute the Longest Common Subsequence of two line arrays.
 * Uses the standard O(n*m) DP approach with backtracking.
 *
 * @param {string[]} a
 * @param {string[]} b
 * @returns {string[]}
 */
function computeLCS(a, b) {
  const n = a.length;
  const m = b.length;
  // dp[i][j] = LCS length of a[0..i-1] and b[0..j-1]
  const dp = new Array(n + 1);
  for (let i = 0; i <= n; i++) {
    dp[i] = new Uint16Array(m + 1);
  }

  for (let i = 1; i <= n; i++) {
    const ai = a[i - 1];
    const row = dp[i];
    const prev = dp[i - 1];
    for (let j = 1; j <= m; j++) {
      if (ai === b[j - 1]) {
        row[j] = prev[j - 1] + 1;
      } else {
        row[j] = Math.max(prev[j], row[j - 1]);
      }
    }
  }

  // Backtrack to reconstruct the LCS.
  const result = [];
  let i = n, j = m;
  while (i > 0 && j > 0) {
    if (a[i - 1] === b[j - 1]) {
      result.push(a[i - 1]);
      i--; j--;
    } else if (dp[i - 1][j] > dp[i][j - 1]) {
      i--;
    } else {
      j--;
    }
  }
  result.reverse();
  return result;
}
