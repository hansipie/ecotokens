# Jev prompts: what ecotokens sends in each scenario

Jev is never given a free-form prompt. Every call is a single HTTPS `POST` of
a JSON body (`src/jev/client.rs::build_request`):

```json
{ "model": "jev-latest", "state": { ... }, "questions": { "<id>": { "type": "...", ... } } }
```

- Endpoint: `https://api.typesafe.ai/v1/systemone` (override with `jev_url`, must be `https`).
- Auth: `Authorization: Bearer $TYPESAFE_API_KEY`.
- `state` is the data being judged. Every string in it is passed through
  `masking::mask` first. If the total string length exceeds
  `jev_max_input_chars`, each string is sampled (40% head, 20% middle,
  40% tail, joined with `\n[…]\n`).
- `questions` maps an id to a typed question:
  - `noul`: yes/no probability, with optional `criteria` `{"true": ..., "false": ...}`
  - `choice`: one option out of `criteria` (`{option: description}`), with probabilities
  - `score`: defined but not used by any caller yet
- Any failure (no key, timeout, HTTP error, malformed or missing answer) makes
  the caller use its built-in heuristic. Transport errors and non-422 HTTP
  errors open a per-process circuit breaker.

## 1. Model router (`src/router/mod.rs::questions`)

Sent for every user message except empty ones and ones starting with `/` or `!`.

- `state`: `{"message": "<user prompt>"}`
- `size` (choice): *What is the smallest AI model size that can do the job asked in `message` well? Judge the work needed to produce a good answer, not the length of the message.*
  - `tiny`: a quick lookup, a rename, a factual question or a one-line answer
  - `everyday`: a normal email, a social post, a short document, a simple explanation or a small self-contained code change
  - `large`: a multi-step build, research, a full report, or code changes across several files
  - `hardest`: strategy, architecture, high-stakes decisions, or anything where a wrong call is expensive
- `needs_conversation` (noul): *Is `message` a short reply that only makes sense inside an ongoing conversation, because it refers to something said earlier (for example 'yes do that but make it shorter' or 'ok go with the second one')?*
  - true: It is a follow-up that depends on earlier messages and cannot be handled on its own.
  - false: It is a self-contained request that can be understood without the conversation.

### Exact request body

```json
{
  "model": "jev-latest",
  "state": {
    "message": "add a --json flag to the gain command"
  },
  "questions": {
    "size": {
      "type": "choice",
      "instructions": "What is the smallest AI model size that can do the job asked in `message` well? Judge the work needed to produce a good answer, not the length of the message.",
      "criteria": {
        "everyday": "everyday: a normal email, a social post, a short document, a simple explanation or a small self-contained code change",
        "hardest": "hardest: strategy, architecture, high-stakes decisions, or anything where a wrong call is expensive",
        "large": "large: a multi-step build, research, a full report, or code changes across several files",
        "tiny": "tiny: a quick lookup, a rename, a factual question or a one-line answer"
      }
    },
    "needs_conversation": {
      "type": "noul",
      "instructions": "Is `message` a short reply that only makes sense inside an ongoing conversation, because it refers to something said earlier (for example 'yes do that but make it shorter' or 'ok go with the second one')?",
      "criteria": {
        "true": "It is a follow-up that depends on earlier messages and cannot be handled on its own.",
        "false": "It is a self-contained request that can be understood without the conversation."
      }
    }
  }
}
```

Notes: `criteria` keys of a `choice` are serialised in alphabetical order
(`BTreeMap`). Only the value of `state.message` changes between calls.

### Decision from the answers (`decide`)

- `needs_conversation` probability >= `router_followup_min_prob`: the main session keeps the message (`self (followup)`).
- Otherwise, `size` confidence < `router_min_confidence`: the main session keeps it (`self (unsure)`).
- Otherwise the message is delegated to `router-tiny`, `router-everyday`, `router-large` or `router-hardest`.

## 2. Generic output filter (`src/filter/generic.rs::filter_generic_with_judge`)

Sent for large command outputs. The head and tail are always kept. The middle
is cut into windows of `JEV_WINDOW_LINES` lines, and every window gets two questions.

- `state`: `window_0`, `window_1`, ... Each value is the window's lines as `"<line_id> <clipped line>"` joined with newlines.
- `pick_k` (choice, one option per line id, described as `Line <id>`): *Which line of `window_k` reports a failure, error, warning, or an identifier (file, test, id) a developer needs to act on this output?*
- `any_k` (noul): *Does any line of `window_k` report a failure, error, or warning?*
  - true: At least one line reports something that went wrong or needs attention
  - false: The lines are routine progress, success, or informational output

If there are too many windows, or the text exceeds `jev_max_input_chars`, Jev is not called.

### Exact request body

Line ids are `L` plus 5 digits. A window holds up to 200 lines (`JEV_WINDOW_LINES`),
and at most 10 windows are sent. The example is cut to one window of 3 lines;
in a real request `pick_0` has one `criteria` entry per line of the window.

```json
{
  "model": "jev-latest",
  "state": {
    "window_0": "L00020 Compiling foo v0.1.0\nL00021 error[E0308]: mismatched types\nL00022 Finished dev profile"
  },
  "questions": {
    "pick_0": {
      "type": "choice",
      "instructions": "Which line of `window_0` reports a failure, error, warning, or an identifier (file, test, id) a developer needs to act on this output?",
      "criteria": {
        "L00020": "Line L00020",
        "L00021": "Line L00021",
        "L00022": "Line L00022"
      }
    },
    "any_0": {
      "type": "noul",
      "instructions": "Does any line of `window_0` report a failure, error, or warning?",
      "criteria": {
        "true": "At least one line reports something that went wrong or needs attention",
        "false": "The lines are routine progress, success, or informational output"
      }
    }
  }
}
```

## 3. Rewrite classification (`src/rewrite/detect.rs::classify_with`)

Used before an auto-rewrite. JSON, diffs, Python tracebacks and Rust panics are
detected without Jev.

- `state`: `{"text": "<trimmed text>"}`
- `kind` (choice): *What kind of content is `text` mostly made of?*
  - `prose`: Natural-language sentences or paragraphs written for a human reader (documentation, explanations, messages), possibly with brief inline code
  - `source_code`: Program source code in any language, even if it contains comments
  - `stack_trace_or_error`: Compiler, runtime, or test-failure output, error messages, or a stack trace
  - `structured_data`: Configuration, key/value pairs, tables, logs, command listings, or other machine-oriented records
  - `mixed`: A substantial mix of prose with code, errors, or data, where rewriting the whole would risk corrupting the non-prose parts

`prose` counts only if its probability is >= `jev_prose_min_prob`.

### Exact request body

```json
{
  "model": "jev-latest",
  "state": {
    "text": "Ecotokens reduces the size of command output before it reaches the model."
  },
  "questions": {
    "kind": {
      "type": "choice",
      "instructions": "What kind of content is `text` mostly made of?",
      "criteria": {
        "mixed": "A substantial mix of prose with code, errors, or data, where rewriting the whole would risk corrupting the non-prose parts",
        "prose": "Natural-language sentences or paragraphs written for a human reader (documentation, explanations, messages), possibly with brief inline code",
        "source_code": "Program source code in any language, even if it contains comments",
        "stack_trace_or_error": "Compiler, runtime, or test-failure output, error messages, or a stack trace",
        "structured_data": "Configuration, key/value pairs, tables, logs, command listings, or other machine-oriented records"
      }
    }
  }
}
```

## 4. Rewrite code gate (`src/rewrite/detect.rs::rewrite_gate`)

One request answering both the code refusal and, for `translate`, the source language.

- `state`: `{"text": "<trimmed text>"}`
- `kind` (choice): same question and options as in section 3. The rewrite is refused only if the answer is `source_code` with probability >= `jev_code_min_prob`.
- `language` (choice, `translate` mode only): *Which natural language is the prose in `text` written in? Ignore code, identifiers, and file paths.*
  - One option per supported language (`LANGUAGES`)
  - An "unclear" option: Several languages in comparable amounts, or the language cannot be determined

### Exact request body

Shown for `translate`, the only mode that adds `language`. For the other modes
the request contains only `kind`. `criteria` keys are in alphabetical order
(`BTreeMap`); the 28 language codes come from `LANGUAGES` in `src/rewrite/modes.rs`.

```json
{
  "model": "jev-latest",
  "state": {
    "text": "Bonjour, voici le rapport de la semaine."
  },
  "questions": {
    "kind": {
      "type": "choice",
      "instructions": "What kind of content is `text` mostly made of?",
      "criteria": {
        "mixed": "A substantial mix of prose with code, errors, or data, where rewriting the whole would risk corrupting the non-prose parts",
        "prose": "Natural-language sentences or paragraphs written for a human reader (documentation, explanations, messages), possibly with brief inline code",
        "source_code": "Program source code in any language, even if it contains comments",
        "stack_trace_or_error": "Compiler, runtime, or test-failure output, error messages, or a stack trace",
        "structured_data": "Configuration, key/value pairs, tables, logs, command listings, or other machine-oriented records"
      }
    },
    "language": {
      "type": "choice",
      "instructions": "Which natural language is the prose in `text` written in? Ignore code, identifiers, and file paths.",
      "criteria": {
        "ar": "Arabic", "cs": "Czech", "da": "Danish", "de": "German", "el": "Greek",
        "en": "English", "es": "Spanish", "fi": "Finnish", "fr": "French", "he": "Hebrew",
        "hi": "Hindi", "hu": "Hungarian", "id": "Indonesian", "it": "Italian", "ja": "Japanese",
        "ko": "Korean",
        "mixed_or_unclear": "Several languages in comparable amounts, or the language cannot be determined",
        "nl": "Dutch", "no": "Norwegian", "pl": "Polish", "pt": "Portuguese", "ro": "Romanian",
        "ru": "Russian", "sv": "Swedish", "th": "Thai", "tr": "Turkish", "uk": "Ukrainian",
        "vi": "Vietnamese", "zh": "Chinese"
      }
    }
  }
}
```

## 5. Rewrite output verification (`src/rewrite/sanitize.rs::judge_response`)

- `state`: `{"mode", "target", "original", "output", "first_line", "last_line"}`
- `faithful` (noul): *`output` should be {a paraphrase of `original` | a rewrite of `original` in a different tone | a rewrite of `original` for a different reading level | a translation of `original`}. Ignoring style, wording, and language, does `output` convey all the meaning of `original` without adding new information, answering it, or leaving parts out? Ignore a leading or trailing line of assistant commentary. Tokens like ⟦ET0⟧ are placeholders and must be treated as opaque.*
- `in_target` (noul, `translate` only): *Is `output` written in {language}? Ignore placeholders, code, names, and numbers.*
- `first_line_commentary` and `last_line_commentary` (noul, only when the output has more than one line): *Is `first_line` (or `last_line`) a remark by an assistant about the text (such as "Here is the rewritten text:" or "Let me know if you need changes"), rather than part of the text itself?*
  - true: The line talks about the task or the text, addressed to the requester
  - false: The line is content belonging to the text

A `faithful` or `in_target` probability below `jev_verify_fail_below` fails the
rewrite. A commentary probability >= `jev_commentary_min_prob` strips that line.

### Exact request body

The fullest case: `translate` mode and a multi-line output, so all four
questions are sent.

```json
{
  "model": "jev-latest",
  "state": {
    "mode": "translate",
    "target": "fr",
    "original": "Here is the weekly report.\nAll tests pass.",
    "output": "Voici le rapport hebdomadaire.\nTous les tests passent.",
    "first_line": "Voici le rapport hebdomadaire.",
    "last_line": "Tous les tests passent."
  },
  "questions": {
    "faithful": {
      "type": "noul",
      "instructions": "`output` should be a translation of `original`. Ignoring style, wording, and language, does `output` convey all the meaning of `original` without adding new information, answering it, or leaving parts out? Ignore a leading or trailing line of assistant commentary. Tokens like ⟦ET0⟧ are placeholders and must be treated as opaque."
    },
    "in_target": {
      "type": "noul",
      "instructions": "Is `output` written in French? Ignore placeholders, code, names, and numbers."
    },
    "first_line_commentary": {
      "type": "noul",
      "instructions": "Is `first_line` a remark by an assistant about the text (such as \"Here is the rewritten text:\" or \"Let me know if you need changes\"), rather than part of the text itself?",
      "criteria": {
        "true": "The line talks about the task or the text, addressed to the requester",
        "false": "The line is content belonging to the text"
      }
    },
    "last_line_commentary": {
      "type": "noul",
      "instructions": "Is `last_line` a remark by an assistant about the text (such as \"Here is the rewritten text:\" or \"Let me know if you need changes\"), rather than part of the text itself?",
      "criteria": {
        "true": "The line talks about the task or the text, addressed to the requester",
        "false": "The line is content belonging to the text"
      }
    }
  }
}
```

Differences by mode:

- `faithful` and `in_target` have no `criteria` (created with `Question::noul`, so the field is omitted).
- Only the `faithful` wording changes between modes: `paraphrase` ("a paraphrase of `original`"), `tone` ("a rewrite of `original` in a different tone"), `reading-level` ("a rewrite of `original` for a different reading level").
- `in_target` exists only for `translate`.
- `target` in `state` is `null` for `paraphrase`.
- The two commentary questions are dropped when the output is a single line.
