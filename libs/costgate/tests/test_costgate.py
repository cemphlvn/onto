import numpy as np
import pytest
from sklearn.datasets import make_classification
from sklearn.model_selection import train_test_split
from sklearn.naive_bayes import GaussianNB
from sklearn.utils.estimator_checks import parametrize_with_checks

from costgate import METHODS, CostGatedClassifier, TopLabelCalibrator, choose_method, gate_cost, threshold


def test_threshold_comes_from_the_costs():
    assert threshold(10, 1) == pytest.approx(0.9)
    assert threshold(1, 1) == 0.0
    assert threshold(0, 1) == 0.0


@pytest.mark.parametrize("method", METHODS)
def test_calibrators_are_monotone_and_bounded(method):
    rng = np.random.default_rng(0)
    c = rng.uniform(0.3, 1.0, 300)
    y = rng.uniform(size=300) < c**3  # overconfident
    cal = TopLabelCalibrator(method=method).fit(c, y)
    grid = np.linspace(0.3, 1.0, 50)
    p = cal.predict(grid)
    assert np.all((p >= 0) & (p <= 1))
    assert np.all(np.diff(p) >= -1e-9)


def test_platt_pulls_an_overconfident_judge_down():
    c = np.full(400, 0.95)
    y = np.arange(400) % 4 == 0  # right one time in four
    p = TopLabelCalibrator(method="platt").fit(c, y).predict([0.95])[0]
    assert abs(p - 0.25) < 0.05


def test_gate_cost_counts_wrong_actions_and_questions():
    prob = np.array([0.95, 0.95, 0.5])
    right = np.array([True, False, True])
    assert gate_cost(prob, right, cost_wrong=10, cost_ask=1) == pytest.approx((0 + 10 + 1) / 3)


def test_choose_method_prefers_the_simplest_within_one_se():
    rng = np.random.default_rng(1)
    c = rng.uniform(0.5, 1.0, 60)
    y = rng.uniform(size=60) < c  # already calibrated, few labels
    method, scores = choose_method(c, y, 10, 1)
    assert method == "identity" or scores[method][0] <= scores["identity"][0]


def test_the_gate_asks_instead_of_erring():
    X, y = make_classification(n_samples=1500, n_features=8, n_informative=4, flip_y=0.15, random_state=0)
    Xtr, Xte, ytr, yte = train_test_split(X, y, test_size=0.5, random_state=0)
    clf = CostGatedClassifier(GaussianNB(), cost_wrong=10, cost_ask=1).fit(Xtr, ytr)
    label, conf, ask = clf.decide(Xte)
    assert label.shape == conf.shape == ask.shape == (len(yte),)
    report = clf.gate_report(Xte, yte)
    plain = np.mean(clf.predict(Xte) != yte) * 10  # act on everything
    assert report["accuracy_of_actions"] > np.mean(clf.predict(Xte) == yte)
    assert report["cost_per_decision"] < plain


@parametrize_with_checks([CostGatedClassifier(cv=3)])
def test_sklearn_compatible(estimator, check):
    check(estimator)
