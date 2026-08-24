import init, { WasmRepl, compile_to_blir, run_program } from './pkg/bloop.js';

await init();
const repl = new WasmRepl();

// ── Tabs ─────────────────────────────────────────────────────────────────

const tabButtons = document.querySelectorAll('.tab-btn');
const panels = document.querySelectorAll('.panel');

tabButtons.forEach((btn) => {
  btn.addEventListener('click', () => {
    tabButtons.forEach((b) => b.classList.toggle('active', b === btn));
    panels.forEach((p) => p.classList.toggle('active', p.id === `panel-${btn.dataset.tab}`));
  });
});

// ── Shared ───────────────────────────────────────────────────────────────

const TAB_INDENT = '    '; // 4 spaces -- a literal '\t' renders far wider

// Tab normally moves focus out of a textarea; both code inputs below want
// it to insert an indent at the cursor instead, like a code editor.
// setRangeText doesn't fire an `input` event on its own, so each caller's
// own listeners (autogrow, debounced compile, ...) are triggered by
// dispatching one manually afterwards.
function handleTabKey(event, textarea) {
  if (event.key !== 'Tab' || event.shiftKey) return false;
  event.preventDefault();
  const { selectionStart, selectionEnd } = textarea;
  textarea.setRangeText(TAB_INDENT, selectionStart, selectionEnd, 'end');
  textarea.dispatchEvent(new Event('input', { bubbles: true }));
  return true;
}

// ── REPL ─────────────────────────────────────────────────────────────────

const output = document.getElementById('output');
const input = document.getElementById('input');
const submitButton = document.getElementById('submit');
const clearButton = document.getElementById('clear');

function appendLine(text, className) {
  const span = document.createElement('span');
  if (className) span.className = className;
  span.textContent = text;
  output.appendChild(span);
  output.scrollTop = output.scrollHeight;
}

// Grows the textarea to fit its content (up to the CSS max-height, where it
// starts scrolling instead), then shrinks it back down when text is removed.
// #input is box-sizing: border-box, so its height must include the border --
// scrollHeight never does, in either box model -- or the box ends up taller
// than its content and leaves a dead gap below the text.
function resizeInput() {
  const style = getComputedStyle(input);
  const border = parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
  input.style.height = 'auto';
  input.style.height = `${input.scrollHeight + border}px`;
}

function submit() {
  const text = input.value;
  if (!text.trim()) return;
  input.value = '';
  resizeInput();
  try {
    const result = repl.eval(text);
    if (result) appendLine(result);
  } catch (err) {
    appendLine(`${err}\n`, 'error');
  }
}

input.addEventListener('input', resizeInput);

input.addEventListener('keydown', (event) => {
  if (handleTabKey(event, input)) return;
  // History recall only makes sense while editing a single line -- once the
  // input spans multiple lines, up/down should move the cursor as usual.
  if (event.key === 'ArrowUp' && !input.value.includes('\n')) {
    const recalled = repl.history_up();
    if (recalled !== undefined) {
      input.value = recalled;
      resizeInput();
    }
    event.preventDefault();
    return;
  }
  if (event.key === 'ArrowDown' && !input.value.includes('\n')) {
    const recalled = repl.history_down();
    if (recalled !== undefined) {
      input.value = recalled;
      resizeInput();
    }
    event.preventDefault();
    return;
  }
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    submit();
  }
  // Shift+Enter falls through to the textarea's default behavior (insert a
  // newline), which fires the 'input' listener above and grows the box.
});

submitButton.addEventListener('click', () => {
  input.focus();
  submit();
});

clearButton.addEventListener('click', () => {
  output.textContent = '';
  input.focus();
});

// ── Compiler ─────────────────────────────────────────────────────────────
//
// Both `compile_to_blir` and `run_program` re-run the whole front end
// (tokenize/parse/type-check/lower) from scratch on every call -- there's
// no incremental state to keep in sync, so each debounced tick just calls
// both and renders whatever comes back.

const compilerSource = document.getElementById('compiler-source');
const compilerBlir = document.getElementById('compiler-blir');
const compilerRunOutput = document.getElementById('compiler-run-output');

const SAMPLE_SOURCE = `fn main() {
    print "Hello world!";
    0
}
`;

function setPaneResult(pane, result) {
  if (result.ok) {
    pane.textContent = result.value;
    pane.classList.remove('error');
  } else {
    pane.textContent = result.error;
    pane.classList.add('error');
  }
}

// wasm-bindgen surfaces a Rust `Result<String, String>` as a resolved value
// on success and a *thrown* string on failure, rather than as some tagged
// object -- normalize both into a plain {ok, value|error} here so the
// render logic above doesn't need its own try/catch per call site.
function callCompiler(fn, source) {
  try {
    return { ok: true, value: fn(source) };
  } catch (err) {
    return { ok: false, error: String(err) };
  }
}

let compileTimer = null;

function compile() {
  compileTimer = null;
  const source = compilerSource.value;
  setPaneResult(compilerBlir, callCompiler(compile_to_blir, source));
  // A run error is almost always the same front-end error already shown in
  // the BLIR pane (compiling failed before execution ever started), but
  // running it again independently also surfaces genuine runtime errors
  // (e.g. division by zero) that compiling alone wouldn't catch.
  setPaneResult(compilerRunOutput, callCompiler(run_program, source));
}

function scheduleCompile() {
  if (compileTimer) clearTimeout(compileTimer);
  compileTimer = setTimeout(compile, 500);
}

compilerSource.addEventListener('input', scheduleCompile);
compilerSource.addEventListener('keydown', (event) => handleTabKey(event, compilerSource));

compilerSource.value = SAMPLE_SOURCE;
compile();
