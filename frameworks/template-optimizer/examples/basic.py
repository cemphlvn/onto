from onto_template_opt import Candidate, Case, Optimizer, Policy

policy = Policy(
    allowed_operations=frozenset({"rewrite"}),
    variables=frozenset({"question"}),
    required_literals=("Answer:",),
    min_cases=3,
)
cases = [Case(str(i), {"question": question}, "yes") for i, question in
         enumerate(("Is this clear?", "Can you answer?", "Is it safe?", "Is it useful?"))]
baseline = Candidate("v0", "{question}\nAnswer:", "rewrite")
variant = Candidate("v1", "Reply clearly: {question}\nAnswer:", "rewrite", parent="v0")

# Replace these two functions with a real application and its independent metric.
def run(rendered, case):
    return "yes" if "Reply clearly:" in rendered else "no"

def score(output, case):
    return float(output == case.reference)

result = Optimizer(policy, run, score).optimize(baseline, [variant], cases)
print(result)
