import unittest

from onto_template_opt import Candidate, Case, Optimizer, Policy


class OptimizerTests(unittest.TestCase):
    def setUp(self):
        self.policy = Policy(frozenset({"rewrite"}), frozenset({"question"}),
                             required_literals=("Answer:",), min_cases=3)
        self.cases = [Case(str(i), {"question": f"Q{i}"}, "yes") for i in range(4)]
        self.base = Candidate("v0", "{question}\nAnswer:", "rewrite")
        self.run = lambda rendered, case: "yes" if "clear" in rendered else "no"
        self.score = lambda output, case: float(output == case.reference)

    def test_promotes_measured_gain(self):
        better = Candidate("v1", "clear {question}\nAnswer:", "rewrite", "v0")
        result = Optimizer(self.policy, self.run, self.score).optimize(self.base, [better], self.cases)
        self.assertEqual(result.incumbent.id, "v1")
        self.assertTrue(result.decisions[0].admitted)

    def test_policy_blocks_missing_literal_without_running(self):
        candidate = Candidate("bad", "clear {question}", "rewrite", "v0")
        result = Optimizer(self.policy, self.run, self.score).optimize(self.base, [candidate], self.cases)
        self.assertEqual(result.incumbent.id, "v0")
        self.assertIn("required literal", result.decisions[0].reason)

    def test_rejects_unsupported_operation_and_stale_parent(self):
        bad = Candidate("bad", "clear {question}\nAnswer:", "change_policy", "v0")
        stale = Candidate("stale", "clear {question}\nAnswer:", "rewrite", "old")
        decisions = Optimizer(self.policy, self.run, self.score).optimize(self.base, [bad, stale], self.cases).decisions
        self.assertFalse(any(d.admitted for d in decisions))

    def test_rejects_attribute_access(self):
        candidate = Candidate("bad", "{question.__class__}\nAnswer:", "rewrite", "v0")
        result = Optimizer(self.policy, self.run, self.score).optimize(self.base, [candidate], self.cases)
        self.assertFalse(result.decisions[0].admitted)


if __name__ == "__main__":
    unittest.main()
