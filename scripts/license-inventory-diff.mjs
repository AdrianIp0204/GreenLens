// Bounded diagnostics for the existing strict inventory comparison. This helper
// does not normalize acceptance, mutate inventories, read files or run tools.
const MAX_DIFFERENCES = 20;
const MAX_SUMMARY_CHARS = 160;
const MAX_PATH_CHARS = 240;
const absolutePath = /(?:^|[\s"'(=])(?:[A-Za-z]:[\\/]|\\\\|\/(?!\/))/;

function clip(text, limit) {
  return text.length <= limit ? text : `${text.slice(0, limit - 1)}…`;
}

function safeText(text) {
  return absolutePath.test(text) ? '[absolute path omitted]' : text;
}

function kind(value) {
  return value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value;
}

function summary(value, limit, missing = false) {
  if (missing) return clip('[missing]', limit);
  const type = kind(value);
  if (type === 'array') return clip(`array (${value.length} entries)`, limit);
  if (type === 'object') return clip(`object (${Object.keys(value).length} keys)`, limit);
  const text = type === 'string' ? JSON.stringify(safeText(value)) : JSON.stringify(value);
  return clip(text, limit);
}

function propertyPath(parent, key) {
  const safeKey = safeText(key);
  const part = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(safeKey)
    ? `${parent ? '.' : ''}${safeKey}` : `[${JSON.stringify(safeKey)}]`;
  return clip(`${parent}${part}`, MAX_PATH_CHARS);
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === 'object') {
    const result = Object.create(null);
    for (const key of Object.keys(value).sort()) result[key] = canonical(value[key]);
    return result;
  }
  return value;
}

function boundedOption(value, fallback, maximum, name) {
  if (value === undefined) return fallback;
  if (!Number.isInteger(value) || value < 1 || value > maximum) {
    throw new Error(`${name} must be an integer from 1 through ${maximum}`);
  }
  return value;
}

/**
 * Inspect JSON inventories without changing the existing JSON.stringify check.
 * Call on the same comparable objects used by the strict gate. `identical` is
 * exactly their JSON.stringify equality; key_order_only never means accepted.
 * Array order remains semantic. Paths are relative JSON property paths, not
 * filesystem paths. Strings containing absolute paths are omitted from output.
 */
export function describeInventoryDifference(expected, actual, options = {}) {
  const limit = boundedOption(options.maxDifferences, MAX_DIFFERENCES, MAX_DIFFERENCES, 'maxDifferences');
  const chars = boundedOption(options.maxSummaryChars, 120, MAX_SUMMARY_CHARS, 'maxSummaryChars');
  const expectedText = JSON.stringify(expected);
  const actualText = JSON.stringify(actual);
  if (expectedText === undefined || actualText === undefined) throw new Error('Inventories must be JSON values');
  const identical = expectedText === actualText;
  const result = { identical, key_order_only: false, differences: [], truncated: false };
  if (identical) return result;
  // Parsing the serialized representation preserves the strict gate's treatment
  // of JSON values, without relying on source object prototypes or toJSON calls.
  const left = JSON.parse(expectedText);
  const right = JSON.parse(actualText);
  result.key_order_only = JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));

  function add(path, type, before, after, beforeMissing = false, afterMissing = false) {
    if (result.differences.length === limit) {
      result.truncated = true;
      return;
    }
    result.differences.push({
      path: path || '[root]', kind: type,
      expected: summary(before, chars, beforeMissing),
      actual: summary(after, chars, afterMissing),
    });
  }

  function visit(before, after, path) {
    if (result.truncated) return;
    const beforeKind = kind(before);
    const afterKind = kind(after);
    if (beforeKind !== afterKind) return add(path, 'type_changed', before, after);
    if (beforeKind === 'array') {
      const length = Math.max(before.length, after.length);
      for (let index = 0; index < length && !result.truncated; index++) {
        const next = clip(`${path}[${index}]`, MAX_PATH_CHARS);
        if (index >= before.length) add(next, 'added', null, after[index], true);
        else if (index >= after.length) add(next, 'removed', before[index], null, false, true);
        else visit(before[index], after[index], next);
      }
    } else if (beforeKind === 'object') {
      const beforeKeys = Object.keys(before);
      const afterKeys = Object.keys(after);
      const sameKeys = beforeKeys.length === afterKeys.length && beforeKeys.every(key => Object.hasOwn(after, key));
      if (sameKeys && beforeKeys.some((key, index) => key !== afterKeys[index])) {
        // The first differing position identifies ordering without dumping all
        // inventory keys or relaxing the comparison for a mixed semantic change.
        const index = beforeKeys.findIndex((key, at) => key !== afterKeys[at]);
        add(path, 'object_key_order', `key ${index}: ${beforeKeys[index]}`, `key ${index}: ${afterKeys[index]}`);
      }
      for (const key of beforeKeys) {
        if (result.truncated) break;
        const next = propertyPath(path, key);
        if (!Object.hasOwn(after, key)) add(next, 'removed', before[key], null, false, true);
        else visit(before[key], after[key], next);
      }
      for (const key of afterKeys) {
        if (result.truncated) break;
        if (!Object.hasOwn(before, key)) add(propertyPath(path, key), 'added', null, after[key], true);
      }
    } else if (before !== after) add(path, 'value_changed', before, after);
  }
  visit(left, right, '');
  return result;
}
