#!/usr/bin/env python3
"""ste-lint - a Simplified Technical English checker for software documentation.

The checker reads Markdown, plain text, or source-code comments and reports the
sentences that break the rules of the simplified-technical-english skill.

It needs Python 3.9 or later. It has no dependencies outside the standard
library.

Usage:
    python3 ste_lint.py docs/*.md
    python3 ste_lint.py --mode procedural RUNBOOK.md
    python3 ste_lint.py --format json --fail-on warning docs/
    python3 ste_lint.py --fix README.md
    python3 ste_lint.py --list-rules
    python3 ste_lint.py --explain STE003
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import re
import sys
from dataclasses import dataclass, field
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

__version__ = "1.0.0"

# --------------------------------------------------------------------------
# Rules
# --------------------------------------------------------------------------

SEVERITY_ORDER = {"info": 0, "warning": 1, "error": 2}


@dataclass(frozen=True)
class Rule:
    code: str
    name: str
    severity: str
    tags: str
    summary: str
    detail: str


RULES: Dict[str, Rule] = {}


def _rule(code, name, severity, tags, summary, detail):
    RULES[code] = Rule(code, name, severity, tags, summary, detail)


_rule(
    "STE001", "sentence-length", "error", "[5.1] [6.3]",
    "The sentence is longer than the budget.",
    "A procedural sentence holds 20 words at most. A descriptive sentence holds\n"
    "25 words at most. Counting follows [8.4]-[8.7]: a number with its unit, an\n"
    "abbreviation, an identifier, quoted text, parenthesized text, and a\n"
    "hyphenated word each count as one word.",
)
_rule(
    "STE002", "passive-voice", "warning", "[3.6]",
    "The sentence uses the passive voice.",
    "Passive voice hides the agent. Name the thing that acts. Passive voice is\n"
    "allowed in descriptive text only when the agent is genuinely unknown.",
)
_rule(
    "STE003", "nominalization", "warning", "[3.7]",
    "An action is written as a noun.",
    "Keep the action in the verb. 'Perform a validation of the payload' becomes\n"
    "'Validate the payload'.",
)
_rule(
    "STE004", "complex-verb", "warning", "[3.2] [3.4]",
    "The verb form is not simple.",
    "Allowed forms: infinitive, imperative, simple present, simple past, simple\n"
    "future, and the past participle used as an adjective. No perfect tenses, no\n"
    "continuous tenses, no conditional chains.",
)
_rule(
    "STE005", "noun-cluster", "warning", "[2.1]",
    "The noun group holds more than three words.",
    "A long chain of nouns forces the reader to guess how the words attach to\n"
    "each other. Write three words at most, or hyphenate the parts that belong\n"
    "together.",
)
_rule(
    "STE006", "semicolon", "error", "[8.1]",
    "The semicolon is not permitted.",
    "The semicolon joins long sentences, which is what the rules prevent. Write\n"
    "two sentences.",
)
_rule(
    "STE007", "latin-abbreviation", "error", "[GR-6]",
    "The text uses a Latin abbreviation.",
    "Latin abbreviations are read differently by different readers, and machine\n"
    "translation handles them badly. Write the words.",
)
_rule(
    "STE008", "contraction", "error", "[4.2]",
    "The text uses a contraction.",
    "Contractions hide a word. Write the full form.",
)
_rule(
    "STE009", "phrasal-verb", "warning", "[9.3]",
    "The text uses a phrasal verb.",
    "The particle changes the meaning of the verb in a way that is not\n"
    "predictable. Non-native readers and machine translation both fail on it.",
)
_rule(
    "STE010", "ambiguous-word", "warning", "[1.11]",
    "The word has more than one meaning in this position.",
    "Choose the word that carries one meaning. See references/word-substitutions.md.",
)
_rule(
    "STE011", "bloat", "warning", "[4.1]",
    "The phrase is longer than it needs to be.",
    "Shorter is clearer. The replacement carries the same meaning.",
)
_rule(
    "STE012", "vague-verb", "info", "[1.11]",
    "The verb does not say what happens.",
    "'Handle', 'process', and 'manage' describe a category of action, not an\n"
    "action. Name the action.",
)
_rule(
    "STE013", "double-negative", "warning", "[4.1]",
    "The sentence states a negative of a negative.",
    "State the positive. The reader then needs one step instead of two.",
)
_rule(
    "STE014", "paragraph-length", "warning", "[6.6]",
    "The paragraph holds more than six sentences.",
    "One topic per paragraph, six sentences at most. Split the paragraph.",
)
_rule(
    "STE015", "condition-order", "warning", "[5.4]",
    "The condition comes after the instruction.",
    "The reader acts before the reader reaches the condition. Put the condition\n"
    "first and separate it with a comma.",
)
_rule(
    "STE016", "ambiguous-this", "warning", "[GR-4]",
    "A demonstrative starts the sentence and points at a clause.",
    "'This can fail' makes the reader reconstruct what 'this' is. Repeat the\n"
    "noun: 'The write operation can fail'.",
)
_rule(
    "STE017", "omitted-that", "warning", "[GR-1]",
    "The conjunction 'that' is missing.",
    "'that' marks where the main clause ends. Many languages cannot omit it, so\n"
    "translation of the sentence becomes ambiguous.",
)
_rule(
    "STE018", "gendered-language", "warning", "[GR-7]",
    "The text uses a gendered pronoun.",
    "Address the reader as 'you'. Use 'they' for an unspecified person, or\n"
    "restructure the sentence to remove the pronoun.",
)
_rule(
    "STE019", "terminology-drift", "info", "[1.11] [9.4]",
    "One concept appears under more than one name.",
    "Choose one term and use it everywhere, in the code and in the prose. Define\n"
    "the choice in the glossary of .ste.json to turn this into an error.",
)

# --------------------------------------------------------------------------
# Word tables
# --------------------------------------------------------------------------

# (pattern, replacement or None, note)
LATIN = [
    (r"\be\.\s?g\.(?:,)?", "for example,", None),
    (r"\bi\.\s?e\.(?:,)?", "that is,", None),
    (r"\betc\.", "and so on", "Name the remaining items instead."),
    (r"\bvs\.?(?=\s)", "compared with", None),
    (r"\bcf\.", "compare", None),
    (r"\bN\.\s?B\.", "Note:", None),
    (r"\bet\s+al\.", "and others", None),
    (r"\bad\s+hoc\b", "unplanned", None),
    (r"\bper\s+se\b", "in itself", None),
    (r"\bvia\b", "through", None),
    (r"\bviz\.", "namely", None),
]

CONTRACTIONS = [
    (r"\bcan't\b", "cannot"), (r"\bwon't\b", "will not"),
    (r"\bdon't\b", "do not"), (r"\bdoesn't\b", "does not"),
    (r"\bdidn't\b", "did not"), (r"\bisn't\b", "is not"),
    (r"\baren't\b", "are not"), (r"\bwasn't\b", "was not"),
    (r"\bweren't\b", "were not"), (r"\bhasn't\b", "has not"),
    (r"\bhaven't\b", "have not"), (r"\bhadn't\b", "had not"),
    (r"\bshouldn't\b", "should not"), (r"\bwouldn't\b", "would not"),
    (r"\bcouldn't\b", "could not"), (r"\bmustn't\b", "must not"),
    (r"\bit's\b", "it is"), (r"\bthat's\b", "that is"),
    (r"\bthere's\b", "there is"), (r"\bhere's\b", "here is"),
    (r"\bwhat's\b", "what is"), (r"\blet's\b", "let us"),
    (r"\byou're\b", "you are"), (r"\bwe're\b", "we are"),
    (r"\bthey're\b", "they are"), (r"\byou'll\b", "you will"),
    (r"\bwe'll\b", "we will"), (r"\bit'll\b", "it will"),
    (r"\byou've\b", "you have"), (r"\bwe've\b", "we have"),
    (r"\bthey've\b", "they have"), (r"\byou'd\b", "you would"),
    (r"\bwe'd\b", "we would"), (r"\bthey'd\b", "they would"),
]

BLOAT = [
    (r"\bin order to\b", "to"),
    (r"\bdue to the fact that\b", "because"),
    (r"\bowing to the fact that\b", "because"),
    (r"\bfor the reason that\b", "because"),
    (r"\bin the event that\b", "if"),
    (r"\bin the case that\b", "if"),
    (r"\bat this point in time\b", "now"),
    (r"\bat the present time\b", "now"),
    (r"\bprior to\b", "before"),
    (r"\bsubsequent to\b", "after"),
    (r"\bin the vicinity of\b", "near"),
    (r"\bwith regard to\b", "about"),
    (r"\bwith respect to\b", "about"),
    (r"\bin terms of\b", "for"),
    (r"\ba large number of\b", "many"),
    (r"\ba small number of\b", "a few"),
    (r"\bthe majority of\b", "most"),
    (r"\bin a timely manner\b", "on time"),
    (r"\bit should be noted that\b", ""),
    (r"\bplease note that\b", ""),
    (r"\bas previously mentioned\b", ""),
    (r"\bin conclusion\b", ""),
    (r"\bneedless to say\b", ""),
    (r"\bfirst of all\b", "first"),
    (r"\bis able to\b", "can"),
    (r"\bare able to\b", "can"),
    (r"\bhas the ability to\b", "can"),
    (r"\bin spite of the fact that\b", "although"),
    (r"\bon a regular basis\b", "regularly"),
    (r"\bof the order of\b", "about"),
]

NOMINALIZATION_PHRASES = [
    (r"\bperform(?:s|ed)? an? (\w+)ation of\b", None, "Use the verb."),
    (r"\bcarr(?:y|ies|ied) out the (\w+)ment of\b", None, "Use the verb."),
    (r"\bmake(?:s)? use of\b", "use", None),
    (r"\bmade use of\b", "used", None),
    (r"\btake into consideration\b", "consider", None),
    (r"\bgive(?:s)? consideration to\b", "consider", None),
    (r"\bdo a comparison of\b", "compare", None),
    (r"\bmake a decision about\b", "decide", None),
    (r"\bprovide a description of\b", "describe", None),
    (r"\beffect a change to\b", "change", None),
    (r"\bprovide(?:s)? support for\b", "supports", None),
    (r"\bis in violation of\b", "breaks", None),
]

PHRASAL = [
    (r"\bset up\b", "configure, install, or prepare"),
    (r"\btear down\b", "remove or stop"),
    (r"\bspin up\b", "start or create"),
    (r"\broll out\b", "deploy or release"),
    (r"\broll back\b", "revert or restore"),
    (r"\bfill in\b", "complete"),
    (r"\bcarry out\b", "do"),
    (r"\blook up\b", "find or search for"),
    (r"\bturn on\b", "enable"),
    (r"\bturn off\b", "disable"),
    (r"\bshut down\b", "stop"),
    (r"\bbring up\b", "start"),
    (r"\btake down\b", "stop or remove"),
    (r"\bfigure out\b", "find or determine"),
    (r"\bend up\b", "become or result in"),
    (r"\bcome up with\b", "propose or write"),
    (r"\bgo through\b", "read or check"),
    (r"\bput in place\b", "add or install"),
    (r"\bkick off\b", "start"),
    (r"\bwipe out\b", "delete"),
    (r"\bhold off\b", "wait"),
]

AMBIGUOUS = [
    (r"\bshould\b", "Write 'must' for a requirement, or 'we recommend that you' for advice."),
    (r"\bmay\b", "Write 'can' for an ability, or 'is permitted to' for permission."),
    (r"\bonce\b", "Write 'after' or 'when'."),
    (r"\bsimply\b", "Delete it. It implies that the reader should already know."),
    (r"\bjust\b", "Delete it. It implies that the reader should already know."),
    (r"\bobviously\b", "Delete it."),
    (r"\bclearly\b", "Delete it."),
    (r"\bcurrently\b", "Delete it, or give a version."),
    (r"\band/or\b", "Write 'A, B, or both'."),
    (r"\bN/A\b", "Write 'not applicable' or 'none'."),
    (r"\bas needed\b", "State the condition."),
    (r"\bif necessary\b", "State the condition."),
    (r"\bas appropriate\b", "State the condition."),
]

VAGUE_VERBS = [
    (r"\bhandle(?:s|d)?\b", "validate, retry, log, reject, or transform"),
    (r"\bprocess(?:es|ed)?\b", "read, parse, write, or transform"),
    (r"\bmanage(?:s|d)?\b", "create, update, delete, or monitor"),
    (r"\bleverage(?:s|d)?\b", "use"),
    (r"\butilize(?:s|d)?\b", "use"),
    (r"\bfacilitate(?:s|d)?\b", "help or allow"),
    (r"\bensure(?:s|d)?\b", "make sure that"),
    (r"\bterminate(?:s|d)?\b", "stop or end"),
    (r"\bcommence(?:s|d)?\b", "start"),
    (r"\binitiate(?:s|d)?\b", "start"),
    (r"\bobtain(?:s|ed)?\b", "get"),
    (r"\bacquire(?:s|d)?\b", "get"),
    (r"\bindicate(?:s|d)?\b", "show"),
    (r"\bassist(?:s|ed)?\b", "help"),
]

DOUBLE_NEGATIVE = [
    (r"\bnot\s+un\w+", "State the positive."),
    (r"\bnot\s+in(?:correct|complete|valid|frequent|significant)\w*", "State the positive."),
    (r"\bdo not fail to\b", "Write the plain instruction."),
    (r"\bunless\b[^.]*\bnot\b", "Rewrite as a positive condition."),
    (r"\bnever\s+not\b", "State the positive."),
]

GENDERED = r"\b(?:he|she|him|his|her|hers|himself|herself|s/he|he/she|his/her)\b"

# Verb-like words that make a leading demonstrative point at a clause.
THIS_VERBS = (
    "is|was|are|were|can|could|will|would|means|meant|causes|caused|allows|"
    "allowed|makes|made|happens|happened|fails|failed|works|worked|does|did|"
    "has|have|had|prevents|breaks|broke|requires|required|leads|led|results|"
    "resulted|affects|affected|avoids|avoided|removes|removed|adds|added|"
    "gives|gave|creates|created|returns|returned|produces|produced"
)

THAT_TRIGGERS = (
    r"\b(make sure|makes sure|ensure|ensures|assume|assumes|assumed|means|"
    r"meant|implies|implied|indicates|indicated|guarantees|guaranteed)\s+"
    r"(the|a|an|you|it|this|these|those|we|they|your|its|all|each|every|no|there)\b"
)

BE = r"(?:is|are|was|were|be|been|being|am)"
ADVERBS = r"(?:not|also|already|then|never|always|only|now|still|automatically|silently|safely|\w+ly)"

IRREGULAR_PARTICIPLES = {
    "done", "made", "sent", "built", "written", "given", "taken", "kept",
    "held", "put", "set", "read", "found", "lost", "left", "chosen", "thrown",
    "drawn", "known", "shown", "grown", "seen", "gone", "begun", "broken",
    "spoken", "run", "cut", "hit", "let", "split", "spread", "shut", "cast",
    "torn", "worn", "won", "paid", "said", "sold", "told", "brought",
    "bought", "caught", "taught", "thought", "sought", "dealt", "meant",
    "felt", "kept", "slept", "swept", "spent", "sent", "lent", "bent",
}

# Past participles that read as plain adjectives. They are not flagged.
PARTICIPIAL_ADJECTIVES = {
    "enabled", "disabled", "required", "deprecated", "based", "related",
    "supported", "expected", "unchanged", "undefined", "unavailable",
    "available", "ready", "done", "closed", "locked", "empty", "missing",
    "present", "limited", "detailed", "advanced", "reserved", "restricted",
    "sorted", "nested", "shared", "signed", "unsigned", "typed", "named",
    "known", "used", "allowed", "permitted", "intended", "designed",
    "supposed", "concerned", "involved", "interested", "pleased", "tired",
}

FUNCTION_WORDS = {
    "a", "an", "the", "and", "or", "but", "if", "when", "then", "than", "that",
    "this", "these", "those", "of", "in", "on", "at", "to", "for", "with",
    "from", "by", "as", "is", "are", "was", "were", "be", "been", "being",
    "am", "do", "does", "did", "has", "have", "had", "can", "could", "will",
    "would", "shall", "should", "may", "might", "must", "not", "no", "yes",
    "it", "its", "you", "your", "we", "our", "they", "their", "he", "she",
    "his", "her", "i", "my", "me", "us", "them", "him", "so", "because",
    "while", "after", "before", "during", "until", "unless", "into", "onto",
    "over", "under", "above", "below", "between", "through", "about",
    "against", "each", "every", "all", "any", "some", "both", "more", "most",
    "less", "least", "only", "also", "very", "such", "same", "other",
    "another", "there", "here", "where", "which", "who", "whom", "whose",
    "what", "how", "why", "now", "always", "never", "often", "again", "up",
    "down", "out", "off", "per", "via", "one", "two", "three", "first",
    "second", "third", "next", "last", "new", "old",
}

# A noun group ends at a verb. The list holds the verb forms that appear most
# often in software documentation, without a part-of-speech tagger.
COMMON_VERBS = {
    "accept", "accepts", "add", "adds", "allow", "allows", "apply", "applies",
    "become", "becomes", "break", "breaks", "build", "builds", "call",
    "calls", "cannot", "carry", "carries", "change", "changes", "check",
    "checks", "choose", "chooses", "come", "comes", "contain", "contains",
    "cost", "costs", "cover", "covers", "create", "creates", "cut", "cuts",
    "define", "defines", "delete", "deletes", "depend", "depends",
    "describe", "describes", "drop", "drops", "end", "ends", "exist",
    "exists", "explain", "explains", "fail", "fails", "find", "finds",
    "fix", "fixes", "follow", "follows", "force", "forces", "get", "gets",
    "give", "gives", "go", "goes", "hide", "hides", "hold", "holds",
    "include", "includes", "keep", "keeps", "know", "knows", "let", "lets",
    "list", "lists", "look", "looks", "lose", "loses", "make", "makes",
    "mark", "marks", "match", "matches", "mean", "means", "meet", "meets",
    "miss", "misses", "move", "moves", "must", "name", "names", "need",
    "needs", "omit", "omits", "open", "opens", "own", "owns", "pass",
    "passes", "pay", "pays", "pick", "picks", "point", "points", "put",
    "puts", "read", "reads", "reach", "reaches", "remove", "removes",
    "repeat", "repeats", "replace", "replaces", "report", "reports",
    "return", "returns", "run", "runs", "save", "saves", "say", "says",
    "see", "sees", "send", "sends", "set", "sets", "show", "shows",
    "split", "splits", "start", "starts", "state", "states", "stay",
    "stays", "stop", "stops", "take", "takes", "tell", "tells", "turn",
    "turns", "use", "uses", "wait", "waits", "want", "wants", "work",
    "works", "write", "writes", "count", "counts", "fire", "fires",
}

UNITS = (
    "ms|s|sec|secs|second|seconds|m|min|mins|minute|minutes|h|hr|hrs|hour|"
    "hours|d|day|days|B|KB|MB|GB|TB|PB|KiB|MiB|GiB|TiB|kb|mb|gb|tb|bytes|"
    "byte|bit|bits|px|pt|em|rem|%|percent|rps|qps|req|reqs|requests|times|"
    "cores|vCPU|CPU|GHz|MHz|Hz|USD|EUR"
)

DRIFT_GROUPS = [
    ("the billing entity", ["tenant", "tenants", "workspace", "workspaces", "org", "orgs", "organisation", "organization"]),
    ("the API address", ["endpoint", "endpoints", "route", "routes"]),
    ("the unit of work", ["job", "jobs", "task", "tasks", "worker", "workers"]),
    ("the settings", ["config", "configuration", "settings", "options"]),
    ("the runtime target", ["environment", "environments", "stage", "stages", "tier", "tiers"]),
    ("the failure", ["error", "errors", "failure", "failures", "fault", "faults"]),
    ("the removal", ["delete", "deletes", "remove", "removes", "destroy", "destroys", "purge", "purges"]),
    ("the read", ["fetch", "fetches", "retrieve", "retrieves", "load", "loads"]),
]

PROCEDURAL_HEADINGS = re.compile(
    r"\b(install|installation|setup|set-up|usage|quick ?start|getting started|"
    r"how to|steps|procedure|procedures|run|running|deploy|deployment|"
    r"troubleshoot|troubleshooting|contributing|upgrade|migration|rollback|"
    r"recovery|runbook|checklist|build|test|release)\b", re.I)

IMPERATIVE_STARTERS = {
    "add", "apply", "build", "call", "change", "check", "choose", "clone",
    "close", "commit", "configure", "confirm", "copy", "create", "delete",
    "deploy", "disable", "do", "download", "edit", "enable", "enter", "exit",
    "export", "find", "fix", "get", "give", "go", "import", "insert",
    "install", "keep", "load", "log", "look", "make", "merge", "move",
    "open", "paste", "pull", "push", "put", "read", "reboot", "remove",
    "rename", "repeat", "replace", "restart", "restore", "revert", "review",
    "run", "save", "scale", "see", "select", "send", "set", "start", "stop",
    "switch", "take", "tell", "type", "update", "upgrade", "use", "verify",
    "wait", "write",
}

# --------------------------------------------------------------------------
# Findings
# --------------------------------------------------------------------------


@dataclass
class Finding:
    path: str
    line: int
    col: int
    code: str
    severity: str
    message: str
    suggestion: Optional[str] = None
    excerpt: Optional[str] = None

    def to_dict(self):
        return {
            "path": self.path, "line": self.line, "column": self.col,
            "code": self.code, "rule": RULES[self.code].name,
            "severity": self.severity, "tags": RULES[self.code].tags,
            "message": self.message, "suggestion": self.suggestion,
            "excerpt": self.excerpt,
        }


# --------------------------------------------------------------------------
# Configuration
# --------------------------------------------------------------------------

DEFAULT_CONFIG = {
    "mode": "auto",
    "max_words_procedural": 20,
    "max_words_descriptive": 25,
    "max_sentences_per_paragraph": 6,
    "max_noun_cluster": 3,
    "disable": [],
    "enable": [],
    "fail_on": "error",
    "glossary": {},
    "allow_words": [],
    "exclude": [],
}


def load_config(start: str) -> dict:
    """Read .ste.json or .ste.toml from `start` upwards to the filesystem root."""
    cfg = dict(DEFAULT_CONFIG)
    directory = os.path.abspath(start if os.path.isdir(start) else os.path.dirname(start) or ".")
    found = None
    while True:
        for name in (".ste.json", ".ste.toml"):
            candidate = os.path.join(directory, name)
            if os.path.isfile(candidate):
                found = candidate
                break
        if found:
            break
        parent = os.path.dirname(directory)
        if parent == directory:
            break
        directory = parent
    if not found:
        return cfg
    try:
        if found.endswith(".json"):
            with open(found, "r", encoding="utf-8") as handle:
                data = json.load(handle)
        else:
            try:
                import tomllib
            except ImportError:  # pragma: no cover - Python 3.10 and older
                sys.stderr.write(
                    "ste-lint: %s needs Python 3.11 or later. The file is ignored.\n" % found)
                return cfg
            with open(found, "rb") as handle:
                data = tomllib.load(handle)
        data = data.get("ste", data) if isinstance(data, dict) else {}
        cfg.update({k: v for k, v in data.items() if k in DEFAULT_CONFIG})
    except (OSError, ValueError) as exc:
        sys.stderr.write("ste-lint: cannot read %s: %s\n" % (found, exc))
    cfg["_path"] = found
    return cfg


# --------------------------------------------------------------------------
# Masking
# --------------------------------------------------------------------------

NUL = "\x00"
_PH_RE = re.compile(r"\x00\d+\x00+")

MASK_PATTERNS = [
    re.compile(r"`[^`\n]*`"),                       # inline code
    re.compile(r"!\[[^\]]*\]\([^)]*\)"),            # image
    re.compile(r"\[([^\]]*)\]\([^)]*\)"),           # link: keep the label
    re.compile(r"<https?://[^>\s]+>"),              # autolink
    re.compile(r"https?://\S+"),                    # bare URL
    re.compile(r"\b[\w.+-]+@[\w-]+\.[\w.]+\b"),     # e-mail address
    re.compile(r"<[^>\s][^>]*>"),                   # HTML tag
    re.compile(r"\{\{[^}]*\}\}"),                   # template placeholder
    re.compile(r"\$\{[^}]*\}"),                     # shell expansion
]

LINK_LABEL_RE = re.compile(r"\[([^\]]*)\]\([^)]*\)")


class Masker:
    """Replace code and links with opaque tokens of the same length.

    The tokens hold no word characters, so a regex rule never matches inside
    them, and every token counts as exactly one word.
    """

    def __init__(self, text: str):
        self.tokens: List[str] = []
        self.text = self._mask(text)

    def _token(self, original: str) -> str:
        index = len(self.tokens)
        self.tokens.append(original)
        core = "%s%d%s" % (NUL, index, NUL)
        if len(original) > len(core):
            return core + NUL * (len(original) - len(core))
        return core

    def _mask(self, text: str) -> str:
        # A link keeps its label, so the prose stays readable. The label is
        # padded back to the original length to keep the columns stable.
        def link_repl(match):
            label = match.group(1)
            pad = len(match.group(0)) - len(label)
            return label + NUL * pad if pad > 0 else label
        text = LINK_LABEL_RE.sub(link_repl, text)
        for pattern in MASK_PATTERNS:
            if pattern is LINK_LABEL_RE:
                continue
            text = pattern.sub(lambda m: self._token(m.group(0)), text)
        return text

    def unmask(self, text: str) -> str:
        def repl(match):
            index = int(match.group(0).strip(NUL) or 0)
            return self.tokens[index] if index < len(self.tokens) else ""
        text = _PH_RE.sub(repl, text)
        return text.replace(NUL, "")


def strip_placeholders(text: str) -> str:
    return _PH_RE.sub("CODE", text).replace(NUL, "")


# --------------------------------------------------------------------------
# Sentence handling
# --------------------------------------------------------------------------

# Words that end with a dot but do not end a sentence. Unit abbreviations such
# as "min" and "sec" are deliberately absent: they often end a sentence.
ABBREV = (r"e\.g|i\.e|etc|vs|cf|approx|Fig|No|al|Inc|Ltd|U\.S|a\.m|p\.m|viz")
_DOT = "\x01"

_PROTECT = [
    (re.compile(r"\b(?:%s)\." % ABBREV), None),
    (re.compile(r"(\d)\.(\d)"), None),
    (re.compile(r"(?<=\w)\.(?=(?:md|py|js|jsx|ts|tsx|json|yml|yaml|toml|sh|"
                r"txt|go|rs|java|rb|php|html|css|scss|xml|csv|log|env|lock|"
                r"cfg|ini|sql|tf|c|h|cpp|swift|kt)\b)"), None),
    (re.compile(r"\b[A-Z]\.(?=[A-Z]\.)"), None),
]

SENTENCE_SPLIT = re.compile(r'(?<=[.!?])["\')\]]?\s+')
LIST_COLON_SPLIT = re.compile(r"(?<=:)\s+")


def protect_dots(text: str) -> str:
    text = _PROTECT[0][0].sub(lambda m: m.group(0).replace(".", _DOT), text)
    text = _PROTECT[1][0].sub(lambda m: m.group(1) + _DOT + m.group(2), text)
    text = _PROTECT[2][0].sub(_DOT, text)
    text = _PROTECT[3][0].sub(lambda m: m.group(0).replace(".", _DOT), text)
    return text


def restore_dots(text: str) -> str:
    return text.replace(_DOT, ".")


def split_sentences(text: str, in_list: bool = False) -> List[Tuple[int, str]]:
    """Split `text` into sentences. Return (offset, sentence) pairs."""
    protected = protect_dots(text)
    out: List[Tuple[int, str]] = []
    position = 0
    for chunk in SENTENCE_SPLIT.split(protected):
        start = protected.find(chunk, position)
        if start < 0:
            start = position
        position = start + len(chunk)
        pieces = [(start, chunk)]
        if in_list:
            pieces = []
            sub_pos = start
            for piece in LIST_COLON_SPLIT.split(chunk):
                sub_start = protected.find(piece, sub_pos)
                if sub_start < 0:
                    sub_start = sub_pos
                sub_pos = sub_start + len(piece)
                pieces.append((sub_start, piece))
        for offset, piece in pieces:
            restored = restore_dots(piece).strip()
            if restored:
                out.append((offset, restored))
    return out


QUOTED = re.compile(r'"[^"\n]*"|“[^”\n]*”')
PARENS = re.compile(r"\([^)\n]*\)")
NUM_UNIT = re.compile(r"\b\d[\d,._]*\s*(?:%s)\b" % UNITS)
WORDISH = re.compile(r"[\w\x00]")


def count_words(sentence: str) -> int:
    """Count words with the STE counting rules [8.4]-[8.7]."""
    text = QUOTED.sub(" QUOTED ", sentence)
    text = PARENS.sub(" ASIDE ", text)
    text = NUM_UNIT.sub(" MEASURE ", text)
    return len([t for t in text.split() if WORDISH.search(t)])


# --------------------------------------------------------------------------
# Markdown segmentation
# --------------------------------------------------------------------------

FENCE = re.compile(r"^\s{0,3}(?:```|~~~)")
HEADING = re.compile(r"^\s{0,3}(#{1,6})\s+(.*)$")
LIST_ITEM = re.compile(r"^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$")
BLOCKQUOTE = re.compile(r"^\s{0,3}>\s?(.*)$")
TABLE_ROW = re.compile(r"^\s*\|")
LINK_DEF = re.compile(r"^\s*\[[^\]]+\]:\s+\S+")
HTML_LINE = re.compile(r"^\s*<[^>]+>\s*$")
DISABLE_FILE = re.compile(r"<!--\s*ste-disable-file([^>]*?)-->")
DISABLE_NEXT = re.compile(r"<!--\s*ste-disable-next-line([^>]*?)-->")
DISABLE_LINE = re.compile(r"<!--\s*ste-disable-line([^>]*?)-->")
CHECKBOX = re.compile(r"^\[[ xX]\]\s*")


@dataclass
class Segment:
    lineno: int
    text: str            # prose, markers removed, masked
    kind: str            # prose | heading | list | table
    mode: str            # procedural | descriptive
    prefix: int          # number of characters removed from the left
    masker: Masker


@dataclass
class Paragraph:
    segments: List[Segment] = field(default_factory=list)
    kind: str = "prose"
    mode: str = "descriptive"


class Block:
    """One or more segments joined into a single text.

    A sentence often runs over several source lines. The rules work on the
    joined text, and `locate` maps a position back to a line and a column.
    """

    def __init__(self, segments: Sequence[Segment], kind: str, mode: str):
        self.segments = list(segments)
        self.kind = kind
        self.mode = mode
        self.starts: List[Tuple[int, Segment]] = []
        parts = []
        position = 0
        for seg in self.segments:
            self.starts.append((position, seg))
            parts.append(seg.text)
            position += len(seg.text) + 1
        self.text = " ".join(parts)

    @property
    def lineno(self) -> int:
        return self.segments[0].lineno

    def locate(self, offset: int) -> Tuple[int, int]:
        chosen = self.starts[0]
        for start, seg in self.starts:
            if start <= offset:
                chosen = (start, seg)
            else:
                break
        start, seg = chosen
        return seg.lineno, seg.prefix + max(0, offset - start) + 1


def _codes_from(raw: str) -> Optional[List[str]]:
    codes = re.findall(r"STE\d{3}", raw.upper())
    return codes or None


def segment_markdown(lines: Sequence[str], default_mode: str) -> Tuple[List[Paragraph], Dict[int, Optional[List[str]]], Optional[List[str]]]:
    """Split Markdown into paragraphs and collect the suppression comments."""
    paragraphs: List[Paragraph] = []
    suppressed: Dict[int, Optional[List[str]]] = {}
    file_disable: Optional[List[str]] = None
    current: Optional[Paragraph] = None
    in_fence = False
    in_frontmatter = False
    heading_mode = default_mode
    explicit_mode: Optional[str] = None
    pending_next: Optional[Tuple[int, Optional[List[str]]]] = None

    def flush():
        nonlocal current
        if current and current.segments:
            paragraphs.append(current)
        current = None

    for index, raw in enumerate(lines):
        lineno = index + 1
        stripped = raw.rstrip("\n")

        if index == 0 and stripped.strip() == "---":
            in_frontmatter = True
            continue
        if in_frontmatter:
            if stripped.strip() == "---":
                in_frontmatter = False
            continue

        match = DISABLE_FILE.search(stripped)
        if match:
            file_disable = _codes_from(match.group(1)) or ["*"]
        match = DISABLE_NEXT.search(stripped)
        if match:
            pending_next = (lineno + 1, _codes_from(match.group(1)) or ["*"])
        match = DISABLE_LINE.search(stripped)
        if match:
            suppressed[lineno] = _codes_from(match.group(1)) or ["*"]
        if pending_next and pending_next[0] == lineno:
            suppressed[lineno] = pending_next[1]
            pending_next = None

        if FENCE.match(stripped):
            in_fence = not in_fence
            flush()
            continue
        if in_fence:
            continue

        marker = re.search(r"<!--\s*ste:\s*(procedural|descriptive|auto)\s*-->", stripped)
        if marker:
            explicit_mode = None if marker.group(1) == "auto" else marker.group(1)
            flush()
            continue

        content = re.sub(r"<!--.*?-->", "", stripped).rstrip()
        if not content.strip():
            flush()
            continue
        if LINK_DEF.match(content) or HTML_LINE.match(content):
            continue
        if re.match(r"^\s{0,3}([-*_])\s*(\1\s*){2,}$", content):
            continue

        heading = HEADING.match(content)
        if heading:
            flush()
            title = heading.group(2).strip()
            heading_mode = "procedural" if PROCEDURAL_HEADINGS.search(title) else default_mode
            prefix = len(content) - len(title)
            paragraphs.append(Paragraph(
                segments=[Segment(lineno, Masker(title).text, "heading",
                                  explicit_mode or heading_mode, prefix, Masker(title))],
                kind="heading", mode=explicit_mode or heading_mode))
            continue

        if TABLE_ROW.match(content):
            body = content.strip()
            if re.match(r"^\|[\s:|-]+\|?$", body):
                continue
            masker = Masker(body)
            if current is None or current.kind != "table":
                flush()
                current = Paragraph(kind="table", mode=explicit_mode or heading_mode)
            current.segments.append(Segment(lineno, masker.text, "table",
                                            current.mode, len(content) - len(body), masker))
            continue

        quote = BLOCKQUOTE.match(content)
        if quote:
            content = quote.group(1)

        item = LIST_ITEM.match(content)
        if item:
            flush()
            body = CHECKBOX.sub("", item.group(2))
            prefix = len(content) - len(body)
            masker = Masker(body)
            mode = explicit_mode
            if mode is None:
                first = re.match(r"([A-Za-z]+)", body)
                mode = ("procedural"
                        if first and first.group(1).lower() in IMPERATIVE_STARTERS
                        else heading_mode)
            current = Paragraph(kind="list", mode=mode)
            current.segments.append(Segment(lineno, masker.text, "list", mode, prefix, masker))
            continue

        body = content.lstrip()
        prefix = len(content) - len(body)
        masker = Masker(body)
        if current is None or current.kind not in ("prose", "list"):
            flush()
            current = Paragraph(kind="prose", mode=explicit_mode or heading_mode)
        current.segments.append(Segment(lineno, masker.text, current.kind,
                                        current.mode, prefix, masker))

    flush()
    return paragraphs, suppressed, file_disable


# --------------------------------------------------------------------------
# The checker
# --------------------------------------------------------------------------


class Linter:
    def __init__(self, config: dict):
        self.config = config
        self.enabled = self._enabled_codes()
        self.allow = {w.lower() for w in config.get("allow_words", [])}

    def _enabled_codes(self):
        disable = set(self.config.get("disable") or [])
        enable = set(self.config.get("enable") or [])
        if enable:
            return {c for c in RULES if c in enable}
        return {c for c in RULES if c not in disable}

    # -- helpers ---------------------------------------------------------

    def _add(self, out, block, offset, code, message, suggestion=None, excerpt=None):
        if code not in self.enabled:
            return
        line, col = block.locate(offset)
        out.append(Finding(
            path="", line=line, col=col, code=code,
            severity=RULES[code].severity, message=message,
            suggestion=suggestion, excerpt=excerpt))

    def _scan(self, out, block, table, code, build_message):
        for entry in table:
            pattern = entry[0]
            for match in re.finditer(pattern, block.text, re.I):
                word = match.group(0)
                if word.lower().strip(".") in self.allow:
                    continue
                message, suggestion = build_message(entry, word)
                self._add(out, block, match.start(), code, message, suggestion, word)

    # -- word-level rules -------------------------------------------------

    def check_words(self, seg: Block) -> List[Finding]:
        out: List[Finding] = []
        text = seg.text

        for match in re.finditer(r";", text):
            self._add(out, seg, match.start(), "STE006",
                      "The semicolon is not permitted. Write two sentences.")

        self._scan(out, seg, LATIN, "STE007",
                   lambda e, w: ("Do not write %r." % w,
                                 e[2] or "Write %r." % e[1]))
        self._scan(out, seg, CONTRACTIONS, "STE008",
                   lambda e, w: ("Do not use the contraction %r." % w,
                                 "Write %r." % e[1]))
        self._scan(out, seg, BLOAT, "STE011",
                   lambda e, w: ("The phrase %r is longer than it needs to be." % w,
                                 "Delete it." if not e[1] else "Write %r." % e[1]))
        self._scan(out, seg, PHRASAL, "STE009",
                   lambda e, w: ("%r is a phrasal verb." % w, "Write %r." % e[1]))
        self._scan(out, seg, VAGUE_VERBS, "STE012",
                   lambda e, w: ("%r does not say what happens." % w,
                                 "Write %s." % e[1]))
        self._scan(out, seg, AMBIGUOUS, "STE010",
                   lambda e, w: ("%r has more than one meaning here." % w, e[1]))
        self._scan(out, seg, DOUBLE_NEGATIVE, "STE013",
                   lambda e, w: ("%r states a negative of a negative." % w, e[1]))
        self._scan(out, seg, NOMINALIZATION_PHRASES, "STE003",
                   lambda e, w: ("%r turns an action into a noun." % w,
                                 e[2] or "Write %r." % e[1]))

        for match in re.finditer(GENDERED, text, re.I):
            self._add(out, seg, match.start(), "STE018",
                      "%r is a gendered pronoun." % match.group(0),
                      "Address the reader as 'you', or use 'they'.", match.group(0))

        for match in re.finditer(THAT_TRIGGERS, text, re.I):
            self._add(out, seg, match.start(), "STE017",
                      "The conjunction 'that' is missing after %r." % match.group(1),
                      "Write '%s that %s'." % (match.group(1), match.group(2)),
                      match.group(0))

        # Generic nominalization: "the validation of", "perform an analysis".
        for match in re.finditer(
                r"\b(?:the|a|an)\s+(\w{4,}(?:tion|sion|ment|ance|ence|ility))\s+of\b",
                text, re.I):
            self._add(out, seg, match.start(), "STE003",
                      "%r turns an action into a noun." % match.group(0),
                      "Use the verb that the noun comes from.", match.group(0))
        for match in re.finditer(
                r"\b(perform|conduct|make|do|provide|give|take|achieve)s?\s+"
                r"(?:a|an|the)\s+(\w{4,}(?:tion|sion|ment|ance|ence))\b", text, re.I):
            self._add(out, seg, match.start(), "STE003",
                      "%r turns an action into a noun." % match.group(0),
                      "Use the verb that the noun comes from.", match.group(0))

        # Complex verb forms.
        for pattern, label in (
                (r"\b(?:has|have|had)\s+(?:%s\s+)?been\b" % ADVERBS, "perfect tense"),
                (r"\b(?:has|have|had)\s+(?:%s\s+)?\w+(?:ed|en)\b" % ADVERBS, "perfect tense"),
                (r"\b(?:is|are|was|were|am)\s+(?:%s\s+)?\w+ing\b" % ADVERBS, "continuous tense"),
                (r"\bwill\s+(?:%s\s+)?(?:have|be)\s+\w+(?:ed|en|ing)\b" % ADVERBS, "compound future"),
                (r"\b(?:would|could|should|might)\s+have\b", "conditional chain"),
                (r"\bis\s+being\b|\bare\s+being\b|\bwas\s+being\b|\bwere\s+being\b",
                 "continuous passive"),
        ):
            for match in re.finditer(pattern, text, re.I):
                self._add(out, seg, match.start(), "STE004",
                          "%r is a %s." % (match.group(0), label),
                          "Use the simple present, the simple past, or the simple future.",
                          match.group(0))

        # Passive voice.
        for match in re.finditer(
                r"\b(%s)\s+(?:%s\s+)?([a-z]+(?:ed|en))\b" % (BE, ADVERBS), text, re.I):
            participle = match.group(2).lower()
            if participle in PARTICIPIAL_ADJECTIVES:
                continue
            if participle.endswith("ed") and len(participle) < 5:
                continue
            tail = text[match.end():match.end() + 4]
            severity_note = " The agent is named after 'by'." if tail.strip().startswith("by") else ""
            self._add(out, seg, match.start(), "STE002",
                      "%r is passive.%s" % (match.group(0), severity_note),
                      "Name the thing that acts, and use the active voice.",
                      match.group(0))
        for match in re.finditer(r"\b(%s)\s+(?:%s\s+)?(%s)\b"
                                 % (BE, ADVERBS, "|".join(sorted(IRREGULAR_PARTICIPLES))),
                                 text, re.I):
            if match.group(2).lower() in PARTICIPIAL_ADJECTIVES:
                continue
            self._add(out, seg, match.start(), "STE002",
                      "%r is passive." % match.group(0),
                      "Name the thing that acts, and use the active voice.",
                      match.group(0))

        return out

    # -- sentence-level rules ---------------------------------------------

    def check_sentence(self, seg: Block, offset: int, sentence: str,
                       mode: str) -> List[Finding]:
        out: List[Finding] = []
        limit = (self.config["max_words_procedural"] if mode == "procedural"
                 else self.config["max_words_descriptive"])
        words = count_words(sentence)
        if words > limit:
            self._add(out, seg, offset, "STE001",
                      "The sentence holds %d words. The %s limit is %d."
                      % (words, mode, limit),
                      "Split the sentence, or cut the words that carry no meaning.",
                      strip_placeholders(sentence)[:120])

        if re.match(r"^(This|That|These|Those)\s+(?:%s)\b" % THIS_VERBS, sentence):
            self._add(out, seg, offset, "STE016",
                      "The sentence starts with a demonstrative that points at a clause.",
                      "Repeat the noun instead.",
                      strip_placeholders(sentence)[:80])

        if mode == "procedural":
            for match in re.finditer(r"[a-z\x00]\s+\b(if|when|unless)\b\s", sentence):
                before = sentence[:match.start()].lower().split()
                if before and before[-1] in {
                        "check", "see", "know", "test", "determine", "ask",
                        "verify", "decide", "tell", "note", "even", "and",
                        "or", "but", "except"}:
                    continue
                self._add(out, seg, offset + match.start(), "STE015",
                          "The condition %r comes after the instruction." % match.group(1),
                          "Move the condition to the front and add a comma.",
                          strip_placeholders(sentence)[:100])
                break

        cluster = self.config.get("max_noun_cluster", 3)
        run: List[str] = []
        start_index = 0

        def flush_run():
            if len(run) > cluster:
                self._add(out, seg, offset + start_index, "STE005",
                          "The noun group %r holds %d words."
                          % (" ".join(run), len(run)),
                          "Write %d words at most, or hyphenate the parts that "
                          "belong together." % cluster, " ".join(run))
            del run[:]

        for token in re.finditer(r"[A-Za-z][A-Za-z-]*|[^\sA-Za-z]+", sentence):
            word = token.group(0)
            low = word.lower()
            if (not word[0].isalpha() or low in FUNCTION_WORDS
                    or low in COMMON_VERBS or word[0].isupper() or "-" in word
                    or len(word) < 3 or low.endswith("ly")):
                flush_run()
                continue
            if not run:
                start_index = token.start()
            run.append(word)
        flush_run()
        return out

    # -- file ------------------------------------------------------------

    def check_text(self, path: str, text: str) -> List[Finding]:
        lines = text.splitlines()
        paragraphs, suppressed, file_disable = segment_markdown(
            lines, "procedural" if self.config["mode"] == "procedural" else "descriptive")

        findings: List[Finding] = []
        for paragraph in paragraphs:
            if paragraph.kind in ("heading", "table"):
                # A heading counts as one word [8.7]. A table row is a label,
                # not a sentence. Both still obey the word-level rules.
                for seg in paragraph.segments:
                    findings.extend(self.check_words(
                        Block([seg], paragraph.kind, paragraph.mode)))
                continue

            block = Block(paragraph.segments, paragraph.kind, paragraph.mode)
            findings.extend(self.check_words(block))
            sentences = split_sentences(block.text, in_list=block.kind == "list")
            for offset, sentence in sentences:
                findings.extend(self.check_sentence(
                    block, offset, sentence, paragraph.mode))
            if (paragraph.kind == "prose"
                    and len(sentences) > self.config["max_sentences_per_paragraph"]):
                self._add(findings, block, 0, "STE014",
                          "The paragraph holds %d sentences. The limit is %d."
                          % (len(sentences), self.config["max_sentences_per_paragraph"]),
                          "Split the paragraph. One topic per paragraph.")

        findings.extend(self.check_terminology(paragraphs))

        for finding in findings:
            finding.path = path
        return self._filter(findings, suppressed, file_disable)

    def check_terminology(self, paragraphs: Iterable[Paragraph]) -> List[Finding]:
        out: List[Finding] = []
        if "STE019" not in self.enabled:
            return out
        counts: Dict[str, int] = {}
        first: Dict[str, Tuple[int, int]] = {}
        for paragraph in paragraphs:
            for seg in paragraph.segments:
                for match in re.finditer(r"[A-Za-z][A-Za-z-]+", seg.text):
                    low = match.group(0).lower()
                    counts[low] = counts.get(low, 0) + 1
                    first.setdefault(low, (seg.lineno, seg.prefix + match.start()))

        glossary = self.config.get("glossary") or {}
        for preferred, banned in glossary.items():
            for word in banned:
                low = word.lower()
                if counts.get(low):
                    line, col = first[low]
                    out.append(Finding(
                        path="", line=line, col=col + 1, code="STE019",
                        severity="error",
                        message="The glossary maps %r to %r." % (word, preferred),
                        suggestion="Write %r everywhere." % preferred,
                        excerpt=word))

        for concept, group in DRIFT_GROUPS:
            present = [(w, counts[w]) for w in group if counts.get(w, 0) >= 2]
            stems = {w.rstrip("s") for w, _ in present}
            if len(stems) >= 2:
                line, col = min(first[w] for w, _ in present)
                names = ", ".join("%s (%d)" % (w, c) for w, c in present)
                out.append(Finding(
                    path="", line=line, col=col + 1, code="STE019",
                    severity="info",
                    message="More than one name for %s: %s." % (concept, names),
                    suggestion="Choose one term and use it everywhere."))
        return out

    def _filter(self, findings, suppressed, file_disable):
        kept = []
        for finding in findings:
            if file_disable and ("*" in file_disable or finding.code in file_disable):
                continue
            codes = suppressed.get(finding.line)
            if codes is not None and ("*" in codes or finding.code in codes):
                continue
            kept.append(finding)
        # Several patterns can match the same words. Report the longest match
        # once, and drop the matches that overlap it.
        spans: Dict[Tuple[int, str], List[Tuple[int, int]]] = {}
        unique = []
        order = sorted(kept, key=lambda f: (f.line, -len(f.excerpt or ""), f.col, f.code))
        for finding in order:
            start = finding.col
            end = start + len(finding.excerpt or "")
            key = (finding.line, finding.code)
            overlaps = spans.setdefault(key, [])
            if any(start < other_end and other_start < end
                   for other_start, other_end in overlaps):
                continue
            overlaps.append((start, end))
            unique.append(finding)
        return sorted(unique, key=lambda f: (f.line, f.col, f.code))


# --------------------------------------------------------------------------
# Automatic fixes
# --------------------------------------------------------------------------

FIXABLE = [(p, r) for p, r, _ in LATIN if r] + CONTRACTIONS + \
          [(p, r) for p, r in BLOAT] + \
          [(p, r) for p, r, _ in NOMINALIZATION_PHRASES if r]


def apply_fixes(text: str) -> Tuple[str, int]:
    """Apply the substitutions that are safe without context."""
    lines = text.splitlines(keepends=True)
    out: List[str] = []
    in_fence = False
    total = 0
    for raw in lines:
        body = raw.rstrip("\n")
        newline = raw[len(body):]
        if FENCE.match(body):
            in_fence = not in_fence
            out.append(raw)
            continue
        if in_fence:
            out.append(raw)
            continue
        masker = Masker(body)
        masked = masker.text
        for pattern, replacement in FIXABLE:
            def repl(match, replacement=replacement):
                nonlocal total
                total += 1
                original = match.group(0)
                if not replacement:
                    return ""
                if original[:1].isupper():
                    return replacement[:1].upper() + replacement[1:]
                return replacement
            masked = re.sub(pattern, repl, masked, flags=re.I)
        fixed = masker.unmask(masked)
        fixed = re.sub(r"  +", " ", fixed)
        fixed = re.sub(r"\s+([.,;:])", r"\1", fixed)
        out.append(fixed.rstrip() + newline if newline else fixed.rstrip())
    return "".join(out), total


# --------------------------------------------------------------------------
# Reporting
# --------------------------------------------------------------------------

COLORS = {"error": "\033[31m", "warning": "\033[33m", "info": "\033[36m",
          "reset": "\033[0m", "dim": "\033[2m", "bold": "\033[1m"}


def report_text(findings: List[Finding], color: bool, quiet: bool) -> str:
    lines = []

    def paint(key, value):
        return "%s%s%s" % (COLORS[key], value, COLORS["reset"]) if color else value

    for finding in findings:
        lines.append("%s:%d:%d: %s %s %s" % (
            finding.path, finding.line, finding.col,
            paint(finding.severity, finding.severity), paint("bold", finding.code),
            finding.message))
        if finding.suggestion and not quiet:
            lines.append("    %s %s" % (paint("dim", "->"), finding.suggestion))
    return "\n".join(lines)


def report_github(findings: List[Finding]) -> str:
    level = {"error": "error", "warning": "warning", "info": "notice"}
    lines = []
    for finding in findings:
        message = finding.message
        if finding.suggestion:
            message += " " + finding.suggestion
        lines.append("::%s file=%s,line=%d,col=%d,title=%s::%s" % (
            level[finding.severity], finding.path, finding.line, finding.col,
            "%s %s" % (finding.code, RULES[finding.code].name),
            message.replace("\n", " ")))
    return "\n".join(lines)


def summarize(findings: List[Finding]) -> Dict[str, int]:
    summary = {"error": 0, "warning": 0, "info": 0}
    for finding in findings:
        summary[finding.severity] += 1
    return summary


# --------------------------------------------------------------------------
# File discovery
# --------------------------------------------------------------------------

TEXT_SUFFIXES = {".md", ".markdown", ".mdx", ".txt", ".rst", ".adoc"}


def collect_paths(inputs: Sequence[str], exclude: Sequence[str]) -> List[str]:
    out: List[str] = []
    for item in inputs:
        if os.path.isdir(item):
            for root, dirs, files in os.walk(item):
                dirs[:] = [d for d in dirs
                           if d not in {".git", "node_modules", ".venv", "venv",
                                        "__pycache__", "dist", "build", ".tox"}]
                for name in sorted(files):
                    if os.path.splitext(name)[1].lower() in TEXT_SUFFIXES:
                        out.append(os.path.join(root, name))
        else:
            out.append(item)
    kept = []
    for path in out:
        if any(fnmatch.fnmatch(path, pattern) or fnmatch.fnmatch(os.path.basename(path), pattern)
               for pattern in exclude):
            continue
        kept.append(path)
    return kept


# --------------------------------------------------------------------------
# Statistics
# --------------------------------------------------------------------------


def file_stats(text: str, config: dict) -> Dict[str, object]:
    paragraphs, _, _ = segment_markdown(text.splitlines(), "descriptive")
    counts = []
    longest = (0, "")
    for paragraph in paragraphs:
        for seg in paragraph.segments:
            if seg.kind in ("heading", "table"):
                continue
            for _, sentence in split_sentences(seg.text, in_list=seg.kind == "list"):
                words = count_words(sentence)
                counts.append(words)
                if words > longest[0]:
                    longest = (words, strip_placeholders(sentence)[:100])
    if not counts:
        return {"sentences": 0, "words": 0, "average": 0.0, "longest": 0, "longest_text": ""}
    return {
        "sentences": len(counts),
        "words": sum(counts),
        "average": round(sum(counts) / len(counts), 1),
        "longest": longest[0],
        "longest_text": longest[1],
    }


# --------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ste-lint",
        description="Check documentation against Simplified Technical English rules.",
        epilog="Rule codes: run --list-rules. Rule detail: run --explain STE003.")
    parser.add_argument("paths", nargs="*", help="files or directories to check")
    parser.add_argument("--mode", choices=("auto", "procedural", "descriptive"),
                        default=None, help="sentence budget to apply (default: auto)")
    parser.add_argument("--format", choices=("text", "json", "github"),
                        default="text", help="output format")
    parser.add_argument("--fail-on", choices=("error", "warning", "info", "never"),
                        default=None, help="lowest severity that sets exit code 1")
    parser.add_argument("--disable", default="", help="comma-separated rule codes to skip")
    parser.add_argument("--enable", default="", help="comma-separated rule codes to run alone")
    parser.add_argument("--exclude", default="", help="comma-separated glob patterns to skip")
    parser.add_argument("--fix", action="store_true",
                        help="rewrite the files with the substitutions that are always safe")
    parser.add_argument("--stats", action="store_true", help="print sentence statistics")
    parser.add_argument("--quiet", action="store_true", help="hide the suggestion lines")
    parser.add_argument("--no-color", action="store_true", help="never color the output")
    parser.add_argument("--list-rules", action="store_true", help="print the rule table and exit")
    parser.add_argument("--explain", metavar="CODE", help="print one rule in full and exit")
    parser.add_argument("--version", action="version", version="ste-lint %s" % __version__)
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.list_rules:
        print("%-8s %-22s %-8s %-14s %s" % ("CODE", "NAME", "SEVERITY", "TAGS", "SUMMARY"))
        for code in sorted(RULES):
            rule = RULES[code]
            print("%-8s %-22s %-8s %-14s %s"
                  % (rule.code, rule.name, rule.severity, rule.tags, rule.summary))
        return 0

    if args.explain:
        code = args.explain.upper()
        if code not in RULES:
            sys.stderr.write("ste-lint: unknown rule %s\n" % code)
            return 2
        rule = RULES[code]
        print("%s  %s  [%s]  %s\n" % (rule.code, rule.name, rule.severity, rule.tags))
        print(rule.summary)
        print()
        print(rule.detail)
        return 0

    if not args.paths:
        parser.print_usage()
        sys.stderr.write("ste-lint: give at least one file or directory\n")
        return 2

    config = load_config(args.paths[0])
    if args.mode:
        config["mode"] = args.mode
    if args.fail_on:
        config["fail_on"] = args.fail_on
    if args.disable:
        config["disable"] = list(config.get("disable", [])) + \
            [c.strip().upper() for c in args.disable.split(",") if c.strip()]
    if args.enable:
        config["enable"] = [c.strip().upper() for c in args.enable.split(",") if c.strip()]
    exclude = list(config.get("exclude", [])) + \
        [p.strip() for p in args.exclude.split(",") if p.strip()]

    paths = collect_paths(args.paths, exclude)
    if not paths:
        sys.stderr.write("ste-lint: no file matched\n")
        return 2

    color = sys.stdout.isatty() and not args.no_color and os.environ.get("NO_COLOR") is None

    if args.fix:
        changed = 0
        replacements = 0
        for path in paths:
            try:
                with open(path, "r", encoding="utf-8") as handle:
                    text = handle.read()
            except OSError as exc:
                sys.stderr.write("ste-lint: cannot read %s: %s\n" % (path, exc))
                return 2
            fixed, count = apply_fixes(text)
            if count and fixed != text:
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(fixed)
                changed += 1
                replacements += count
                print("fixed %s (%d replacements)" % (path, count))
        print("ste-lint: %d replacements in %d files" % (replacements, changed))
        return 0

    linter = Linter(config)
    findings: List[Finding] = []
    stats_rows = []
    for path in paths:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                text = handle.read()
        except (OSError, UnicodeDecodeError) as exc:
            sys.stderr.write("ste-lint: cannot read %s: %s\n" % (path, exc))
            return 2
        findings.extend(linter.check_text(path, text))
        if args.stats:
            row = file_stats(text, config)
            row["path"] = path
            stats_rows.append(row)

    summary = summarize(findings)

    if args.format == "json":
        print(json.dumps({
            "version": __version__,
            "files": len(paths),
            "summary": summary,
            "findings": [f.to_dict() for f in findings],
            "stats": stats_rows if args.stats else None,
        }, indent=2))
    elif args.format == "github":
        output = report_github(findings)
        if output:
            print(output)
    else:
        output = report_text(findings, color, args.quiet)
        if output:
            print(output)
        if args.stats:
            print()
            print("%-40s %9s %9s %9s %9s"
                  % ("FILE", "SENTENCES", "WORDS", "AVERAGE", "LONGEST"))
            for row in stats_rows:
                print("%-40s %9d %9d %9.1f %9d"
                      % (row["path"][-40:], row["sentences"], row["words"],
                         row["average"], row["longest"]))
        print()
        print("ste-lint: %d files, %d errors, %d warnings, %d notes"
              % (len(paths), summary["error"], summary["warning"], summary["info"]))

    fail_on = config.get("fail_on", "error")
    if fail_on == "never":
        return 0
    threshold = SEVERITY_ORDER[fail_on]
    if any(SEVERITY_ORDER[f.severity] >= threshold for f in findings):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
