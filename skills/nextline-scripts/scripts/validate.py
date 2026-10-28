#!/usr/bin/env python3
"""Validate a NextLine script file. Standard library only.

Usage: python validate.py FILE.json [FILE2.json ...]
Exit status: 0 = no errors (warnings may be printed), 1 = at least one error.
"""
import json
import re
import sys

AFTER_KEYS = {
    "none", "enter", "tab", "shiftTab", "escape", "space", "backspace", "delete",
    "arrowUp", "arrowDown", "arrowLeft", "arrowRight", "ctrlEnter", "shiftEnter",
}
SECRET_HINTS = re.compile(r"(password|passwd|api[_-]?key|secret|bearer\s+[a-z0-9]|token\s*[:=]|sk-[a-z0-9]{10,})", re.I)
EMAIL = re.compile(r"[\w.+-]+@([\w-]+\.)+[a-z]{2,}", re.I)
SAFE_EMAIL_DOMAINS = ("example.com", "example.org", "example.net")


def scripts_of(data):
    if isinstance(data, list):
        return data
    if isinstance(data, dict):
        if isinstance(data.get("scripts"), list):
            return data["scripts"]
        return [data]
    return None


def check_file(path):
    errors, warnings = [], []
    try:
        with open(path, "r", encoding="utf-8-sig") as handle:
            data = json.load(handle)
    except FileNotFoundError:
        return [f"{path}: file not found"], []
    except json.JSONDecodeError as exc:
        return [f"{path}: not valid JSON ({exc})"], []

    scripts = scripts_of(data)
    if not scripts:
        return [f"{path}: expected {{\"scripts\": [...]}}, an array of scripts, or one script object"], []

    seen_titles = set()
    for si, script in enumerate(scripts, 1):
        where = f"script {si}"
        if not isinstance(script, dict):
            errors.append(f"{where}: must be an object")
            continue
        title = script.get("title", script.get("name"))
        if not isinstance(title, str) or not title.strip():
            errors.append(f"{where}: missing a non-empty \"title\"")
        else:
            where = f"script {si} (\"{title.strip()}\")"
            if title.strip().lower() in seen_titles:
                warnings.append(f"{where}: duplicate title; give each script a unique title")
            seen_titles.add(title.strip().lower())
        if "category" in script and not isinstance(script["category"], str):
            errors.append(f"{where}: \"category\" must be a string")
        elif not str(script.get("category", "")).strip():
            warnings.append(f"{where}: no category (it will be filed under \"Imported\")")

        steps = script.get("steps", script.get("items"))
        if not isinstance(steps, list) or not steps:
            errors.append(f"{where}: needs a non-empty \"steps\" array")
            continue
        if len(steps) > 25:
            warnings.append(f"{where}: {len(steps)} steps is a lot for one demo; consider splitting")

        for ti, step in enumerate(steps, 1):
            at = f"{where}, step {ti}"
            if not isinstance(step, dict):
                errors.append(f"{at}: must be an object")
                continue
            body = step.get("body", step.get("text", ""))
            if not isinstance(body, str):
                errors.append(f"{at}: \"body\" must be a string")
                body = ""
            key = step.get("afterKey", "none")
            if key not in AFTER_KEYS:
                errors.append(f"{at}: afterKey \"{key}\" is not valid. Use one of: {', '.join(sorted(AFTER_KEYS))}")
                key = "none"
            if "note" in step and not isinstance(step["note"], str):
                errors.append(f"{at}: \"note\" must be a string")
            if "delayMs" in step:
                delay = step["delayMs"]
                if isinstance(delay, bool) or not isinstance(delay, int) or not 0 <= delay <= 60000:
                    errors.append(f"{at}: \"delayMs\" must be an integer from 0 to 60000")

            if not body.strip() and key == "none":
                errors.append(f"{at}: empty step (no text and no afterKey), so nothing would happen")
            if len(body) > 300:
                warnings.append(f"{at}: {len(body)} characters is long (about {len(body) // 25}s on screen); consider splitting")
            if "\n" in body or "\r" in body:
                warnings.append(f"{at}: contains a newline, typed as Shift+Enter in chat boxes. Intended?")
            if "\t" in body:
                warnings.append(f"{at}: contains a tab character, typed as the Tab key. Intended?")
            if body != body.strip() and body.strip():
                warnings.append(f"{at}: leading or trailing spaces will be typed")
            if SECRET_HINTS.search(body):
                warnings.append(f"{at}: looks like it may contain a secret. Scripts are plain text; do not store credentials")
            for match in EMAIL.finditer(body):
                if not match.group(0).lower().endswith(SAFE_EMAIL_DOMAINS):
                    warnings.append(f"{at}: email address \"{match.group(0)}\" may be real; prefer an @example.com address")

        last = steps[-1] if isinstance(steps[-1], dict) else {}
        if last.get("afterKey") == "tab":
            warnings.append(f"{where}: the last step ends with Tab; use \"none\" so the presenter decides when to submit")

    return errors, warnings


def main(argv):
    if not argv:
        print(__doc__.strip())
        return 2
    failed = False
    for path in argv:
        errors, warnings = check_file(path)
        for message in errors:
            print(f"ERROR   {message}")
        for message in warnings:
            print(f"warning {message}")
        if errors:
            failed = True
        else:
            print(f"OK      {path}" + (f" ({len(warnings)} warning(s))" if warnings else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
