---
name: nextline-scripts
description: Create, review or convert demo scripts for NextLine, a Windows app that types pre-written steps into the focused app on a hotkey. Use when the user wants to author a NextLine script or JSON file, turn a demo plan, chat transcript, prompt list or form walkthrough into steps, add presenter notes, or fix a NextLine import that failed.
---

# NextLine script author

NextLine types one **step** at a time into whatever app has focus (a chat box, a web form, a document). The presenter
presses a hotkey, the step is typed with human-looking pacing, an optional key is pressed afterwards (usually Enter or
Tab), and NextLine moves on to the next step. Your job is to produce a script file the presenter can **Import**.

## Workflow

1. **Find out what is being demoed.** Ask only for what you cannot infer:
   - Where will the text go: a chat/agent prompt, a multi-field form, a search box, something else?
   - Roughly how long should the demo run, and who is the audience?
   - Any facts, product names or wording that must appear? Never invent product capabilities.
2. **Draft the steps** using the rules below.
3. **Write the JSON file** (for example `my-demo.nextline.json`) in the format below.
4. **Validate it**: run `python scripts/validate.py my-demo.nextline.json` from this skill's folder (or give the full path to
   `validate.py`). Fix every error and read every warning.
5. **Tell the user how to load it**: in NextLine press **Import** (bottom left) and pick the file. Imported scripts are added to the
   top of the library with fresh IDs, so importing twice never overwrites anything.

## File format

```json
{
  "scripts": [
    {
      "title": "Order status chatbot",
      "category": "Chatbots",
      "steps": [
        {
          "body": "Hi! Where is order #48213? It was due yesterday.",
          "afterKey": "enter",
          "note": "Pause here and point out the order lookup."
        }
      ]
    }
  ]
}
```

A top-level array of scripts, or a single script object, is also accepted. IDs are optional; NextLine creates them.

| Field | Required | Meaning |
| --- | --- | --- |
| `title` | yes | Name shown in the library. Short and specific. |
| `category` | no | Groups scripts in the sidebar (`Chatbots`, `Forms`, `Agents`...). Defaults to `Imported`. |
| `steps` | yes | Ordered list, at least one. |
| `steps[].body` | yes | The exact text to type. May be empty only if `afterKey` is set. |
| `steps[].afterKey` | no | Key pressed after typing. Default `none`. See below. |
| `steps[].note` | no | Private presenter note. Shown on screen to the presenter, **never typed**. |
| `steps[].delayMs` | no | Extra wait (0 to 60000 ms) before this step starts typing. |

`afterKey` values (exact, case-sensitive): `none`, `enter`, `tab`, `shiftTab`, `escape`, `space`, `backspace`, `delete`,
`arrowUp`, `arrowDown`, `arrowLeft`, `arrowRight`, `ctrlEnter`, `shiftEnter`.

The full machine-readable definition is in [`schema/nextline-script.schema.json`](schema/nextline-script.schema.json), and
[`examples/`](examples) has complete, valid scripts.

## How text is typed (this drives how you write steps)

- Characters are sent as real keystrokes, so emoji and any language work, and the clipboard is untouched.
- A **newline** inside `body` is typed as **Shift+Enter** by default (safe in chat boxes where Enter sends). A **tab character** is typed
  as the Tab key. Avoid both inside chat prompts unless a multi-line message is genuinely wanted.
- Typing is paced like a person: pauses at sentence ends and commas. Long steps take long. At the default speed, expect about
  25 characters per second, so a 200-character step takes roughly 8 seconds on screen.

## Rules for good scripts

**Choosing `afterKey`**
- Chat or agent prompt: `enter` (sends the message).
- Form fields: `tab` after every field except the last; use `none` on the final field so the presenter chooses when to submit.
- A search box: `enter`. A dropdown or list to pick: `arrowDown` then `enter` as separate steps.
- A step that only presses a key (for example submit the form): leave `body` empty and set `afterKey`.
- When unsure, use `none`. A surprising extra keypress is worse than pressing Enter by hand.

**Writing the steps**
- One idea per step. Aim for 40 to 200 characters; split anything over 300.
- Write what a real person would type: natural, slightly casual, no markdown, no stage directions inside `body`.
- Order matters: build a story. Open simple, add a follow-up that builds on the previous answer, finish with the "wow" request.
- Put presenter guidance in `note` ("Wait for the answer", "Click the Sources tab"), never in `body`.
- Use `delayMs` sparingly, only when the audience needs time to read the previous output.
- 4 to 8 steps suits a 5-minute demo.

**Safety and quality**
- Use obviously fictional data (`Avery Stone`, `avery.stone@example.com`, `Harbor & Pine Co.`). Never real customer data.
- Never put passwords, API keys, tokens or other secrets in a script. The file is plain text.
- Do not claim product features the user did not mention. If a step depends on the demo environment, say so in `note`.
- Keep `title` unique and descriptive; put the audience or scenario in `category` or `title`.

## Converting existing material

- **A chat transcript:** keep only the user's turns, tidy typos, one step each with `afterKey: "enter"`; move the assistant's expected answer into `note`.
- **A form walkthrough:** one step per field in tab order, `tab` between them, `none` on the last.
- **A bullet list of prompts:** one step per bullet; add `enter` for chat targets.
- **An export from an earlier version:** it already imports. Legacy `items`, `name` and `text` field names are understood.

## Checklist before you hand it over

- [ ] Valid JSON, `validate.py` reports no errors
- [ ] Every chat step ends with `enter`; every form step except the last ends with `tab`
- [ ] No secrets, real names or real customer data
- [ ] Presenter notes explain anything the presenter must do between steps
- [ ] The user knows to use **Import** in NextLine
