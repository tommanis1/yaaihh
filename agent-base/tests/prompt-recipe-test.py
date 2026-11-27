#!/usr/bin/env python3
"""prompt-recipe's composing (owner's Stream 4A, N-1979): the pinned base less the recipe's cuts plus the model's packs
and the standing text, each part a segment; a cut that matches nothing is said, never dropped silently."""
import importlib.machinery
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
import unittest

BIN = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "bin", "prompt-recipe")
loader = importlib.machinery.SourceFileLoader("prompt_recipe", BIN)
spec = importlib.util.spec_from_loader("prompt_recipe", loader)
pr = importlib.util.module_from_spec(spec)
loader.exec_module(pr)

MEM = "/cfg/projects/-w/memory/"


def labels(diff, kind):
    return [s["label"] for s in diff["segments"] if s["kind"] == kind]


class Compose(unittest.TestCase):
    def test_the_recipes_cuts_all_match_the_pinned_base_and_take_only_their_parts(self):
        out, diff, how, notes = pr.compose("claude-code", "claude-opus-5-5", "STANDING", "2.1.332", MEM)
        self.assertEqual(how, "replace")
        self.assertEqual(notes, [])
        self.assertEqual(labels(diff, "stripped"), ["#4 Pronouns", "#7 Session-specific guidance", "#9 Product facts", "#11 Games", "#12 EndConversation"])
        for gone in ("When you use a pronoun", "# Session-specific guidance", "ultrareview", "The most recent Claude models", "Do build original games", "EndConversation (deferred tool)"):
            self.assertNotIn(gone, out)
        for kept in ("# Harness", "For actions that are hard to reverse", "You must treat information", "# Memory", "# Context management"):
            self.assertIn(kept, out)
        self.assertIn(f"memory at `{MEM}`", out)
        self.assertTrue(out.rstrip("\n").endswith("STANDING"))

    def test_kept_and_added_segments_are_the_text_in_order_and_the_totals_add_up(self):
        out, diff, _, _ = pr.compose("claude-code", "claude-fable-5-2", "STANDING", "2.1.332", MEM)
        parts = [s["text"] for s in diff["segments"] if s["kind"] in ("kept", "added")]
        self.assertEqual("\n\n".join(parts) + "\n", out)
        for k in ("kept", "stripped", "added"):
            self.assertEqual(diff["tokens"][k], sum(s["tokens"] for s in diff["segments"] if s["kind"] == k))
        self.assertEqual(labels(diff, "added"), ["pack claude", "pack fable", "standing text"])

    def test_a_cut_an_update_reworded_is_missing_and_said_and_its_text_stays(self):
        base = os.path.join(pr.PROMPTS, "base", "claude-code-2.1.332.md")
        with open(base) as f:
            text = f.read()
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "base"))
            shutil.copytree(os.path.join(pr.PROMPTS, "recipes"), os.path.join(d, "recipes"))
            shutil.copytree(os.path.join(pr.PROMPTS, "packs"), os.path.join(d, "packs"))
            with open(os.path.join(d, "base", "claude-code-9.9.9.md"), "w") as f:
                f.write(text.replace("Do build original games", "Build original games"))
            was, pr.PROMPTS = pr.PROMPTS, d
            try:
                out, diff, how, notes = pr.compose("claude-code", "claude-opus-5-5", "", "9.9.9", MEM)
            finally:
                pr.PROMPTS = was
        self.assertEqual(how, "replace")
        self.assertEqual(labels(diff, "missing"), ["#11 Games"])
        self.assertEqual(len(notes), 1)
        self.assertIn("#11 Games", notes[0])
        self.assertIn("Build original games", out)

    def test_memory_off_cuts_the_whole_memory_section_and_nothing_after_it(self):
        out, diff, _, _ = pr.compose("claude-code", "claude-opus-5-5", "", "2.1.332", None)
        self.assertIn("#8 Memory", labels(diff, "stripped"))
        self.assertNotIn("# Memory", out)
        self.assertNotIn("{{memory_dir}}", out)
        self.assertIn("# Context management", out)

    def test_a_version_with_no_base_appends_the_packs_and_says_so(self):
        out, diff, how, notes = pr.compose("claude-code", "claude-opus-5-5", "STANDING", "0.0.1", MEM)
        self.assertEqual(how, "append")
        self.assertIn("no base text for claude-code 0.0.1", notes[0])
        self.assertEqual(labels(diff, "kept") + labels(diff, "stripped"), [])
        self.assertTrue(out.startswith("# Working rules"))

    def test_each_model_gets_its_familys_pack_and_an_unknown_one_none(self):
        self.assertEqual(pr.packs_for("claude-fable-5-2"), ["claude", "fable"])
        self.assertEqual(pr.packs_for("claude-opus-5-5"), ["claude"])
        self.assertEqual(pr.packs_for("gpt-6.1-sol"), ["codex"])
        self.assertEqual(pr.packs_for("agy/gemini-4-pro"), ["agy"])
        self.assertEqual(pr.packs_for("qwen-9"), [])
        _, diff, how, notes = pr.compose("codex", "qwen-9", "S", None, None)
        self.assertEqual((how, notes), ("append", ["no pack for model qwen-9"]))

    def test_a_role_pack_comes_after_the_models_and_one_that_is_not_there_is_refused(self):
        out, diff, _, _ = pr.compose("claude-code", "claude-fable-5-2", "", "2.1.332", MEM, ["fable-owner", "fable"])
        self.assertEqual(diff["packs"], ["claude", "fable", "fable-owner"])
        self.assertIn("# Ownership", out)
        with self.assertRaises(OSError):
            pr.compose("claude-code", "claude-fable-5-2", "", "2.1.332", MEM, ["no-such-pack"])

    def test_a_pack_harness_or_version_that_is_a_path_is_refused_before_any_file_is_read(self):
        for extra, harness, version in ((["../base/claude-code-2.1.332"], "claude-code", "2.1.332"), (["a/b"], "claude-code", "2.1.332"),
                                        (["x..y"], "claude-code", "2.1.332"), ([], "../recipes/claude-code", None),
                                        ([], "claude-code", "2.1.332/../../x"), ([".hidden"], "claude-code", "2.1.332")):
            with self.assertRaises(ValueError, msg=(extra, harness, version)):
                pr.compose(harness, "claude-opus-5-5", "", version, MEM, extra)

    def test_a_link_out_of_prompts_is_refused(self):
        link = os.path.join(pr.PROMPTS, "packs", "zz-test-link.md")
        os.symlink("/etc/hostname", link)
        try:
            with self.assertRaises(ValueError):
                pr.compose("codex", "gpt-6.1-sol", "", None, None, ["zz-test-link"])
        finally:
            os.unlink(link)

    def test_the_diff_carries_the_sha_roster_records_of_the_text(self):
        out, diff, _, _ = pr.compose("claude-code", "claude-opus-5-5", "S\n\n", "2.1.332", MEM)
        sh = subprocess.run(["sh", "-c", 'printf "%s" "$(cat)" | sha256sum | cut -c1-64'], input=out, capture_output=True, text=True).stdout.strip()
        self.assertEqual(diff["system_sha"], sh)

    def test_the_packs_carry_no_other_familys_words(self):
        for name, foreign in (("codex", ("Claude", "Skill", "TodoWrite")), ("agy", ("Claude", "Skill", "CLAUDE.md", "Codex"))):
            with open(os.path.join(pr.PROMPTS, "packs", f"{name}.md")) as f:
                text = f.read()
            for w in foreign:
                self.assertNotIn(w, text, f"{name}.md names {w}")

    def test_the_memory_folder_is_named_as_claude_code_names_it(self):
        self.assertEqual(pr.memory_dir("/home/t/.claude", "/home/t/Documents/papers/soft-swarm"),
                         "/home/t/.claude/projects/-home-t-Documents-papers-soft-swarm/memory/")
        self.assertEqual(pr.memory_dir("/c", "/a/b.c_d"), "/c/projects/-a-b-c-d/memory/")


class Command(unittest.TestCase):
    def test_the_command_writes_the_text_and_its_diff_and_says_how_first(self):
        with tempfile.TemporaryDirectory() as d:
            out = os.path.join(d, "system-prompt.md")
            r = subprocess.run([BIN, "claude-code", "claude-opus-5-5", "-", out, "--version", "2.1.332", "--config-dir", "/cfg", "--cwd", "/w"],
                               input="STANDING", capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(r.stdout, "replace\n")
            with open(out + ".diff.json") as f:
                diff = json.load(f)
            with open(out) as f:
                self.assertIn(f"memory at `{MEM}`", f.read())
            self.assertEqual((diff["harness"], diff["version"], diff["how"]), ("claude-code", "2.1.332", "replace"))

    def test_a_write_that_fails_leaves_no_temporary_file_and_says_why(self):
        with tempfile.TemporaryDirectory() as d:
            out = os.path.join(d, "system-prompt.md")
            os.mkdir(out + ".diff.json")
            r = subprocess.run([BIN, "codex", "gpt-6.1-sol", "-", out], input="S", capture_output=True, text=True)
            self.assertEqual(r.returncode, 1)
            self.assertIn("prompt-recipe:", r.stderr)
            self.assertEqual(sorted(os.listdir(d)), ["system-prompt.md", "system-prompt.md.diff.json"])

    def test_a_claude_code_base_without_its_memory_folder_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            r = subprocess.run([BIN, "claude-code", "claude-opus-5-5", "-", os.path.join(d, "o"), "--version", "2.1.332"],
                               input="", capture_output=True, text=True)
            self.assertEqual(r.returncode, 2)
            self.assertFalse(os.path.exists(os.path.join(d, "o")))


if __name__ == "__main__":
    unittest.main()
