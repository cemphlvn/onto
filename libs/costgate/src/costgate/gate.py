"""A classifier that knows when to ask.

`CostGatedClassifier` wraps any probabilistic classifier, calibrates its
confidence in the class it picks from out-of-fold predictions, and turns
the developer's costs into a decision rule: act when the calibrated
probability of being right reaches 1 - cost_ask / cost_wrong, ask a
person otherwise.
"""

import numpy as np
from sklearn.base import BaseEstimator, ClassifierMixin, MetaEstimatorMixin, clone
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import StratifiedKFold, check_cv, cross_val_predict
from sklearn.utils.validation import check_is_fitted, validate_data

from .calibrators import METHODS, TopLabelCalibrator, gate_cost, threshold


def choose_method(confidence, correct, cost_wrong, cost_ask, methods=METHODS, n_splits=5, random_state=0):
    """The calibration method for the gate, chosen on what the gate
    decides: mean cost per decision on held-out folds. The simplest
    method (in `methods` order) within one standard error of the best
    wins, so a few labels do not buy a flexible map.

    Returns (method, {method: (mean cost, standard error)}).
    """
    c = np.asarray(confidence, dtype=float)
    y = np.asarray(correct, dtype=bool)
    if len(np.unique(y)) < 2 or len(c) < 2 * n_splits:
        return methods[0], {}
    folds = StratifiedKFold(n_splits=n_splits, shuffle=True, random_state=random_state)
    scores = {}
    for m in methods:
        costs = []
        for train, test in folds.split(c.reshape(-1, 1), y):
            cal = TopLabelCalibrator(method=m).fit(c[train], y[train])
            costs.append(gate_cost(cal.predict(c[test]), y[test], cost_wrong, cost_ask))
        costs = np.array(costs)
        scores[m] = (float(costs.mean()), float(costs.std(ddof=1) / np.sqrt(len(costs))))
    best = min(scores, key=lambda m: scores[m][0])
    bar = scores[best][0] + scores[best][1]
    return next(m for m in methods if scores[m][0] <= bar), scores


class CostGatedClassifier(MetaEstimatorMixin, ClassifierMixin, BaseEstimator):
    """Act or ask, by calibrated confidence and the costs of each.

    Parameters
    ----------
    estimator : classifier with predict_proba, default LogisticRegression()
    method : {"auto", "identity", "platt", "isotonic", "venn_abers"}
        "auto" chooses by gate cost on held-out folds (`choose_method`).
    cost_wrong, cost_ask : float
        The cost of acting on a wrong prediction and of asking a person,
        in the same unit. Only their ratio matters.
    cv : int or splitter
        Folds for the out-of-fold predictions the calibration is fitted on.

    Attributes
    ----------
    estimator_ : the estimator refitted on all data.
    calibrator_ : the fitted TopLabelCalibrator.
    method_ : the method used.
    threshold_ : 1 - cost_ask / cost_wrong.
    method_scores_ : gate cost per method when method="auto".

    `predict` returns the class like any classifier; `decide` adds the
    calibrated confidence and whether to ask.
    """

    def __init__(self, estimator=None, *, method="auto", cost_wrong=10.0, cost_ask=1.0, cv=5):
        self.estimator = estimator
        self.method = method
        self.cost_wrong = cost_wrong
        self.cost_ask = cost_ask
        self.cv = cv

    def fit(self, X, y):
        X, y = validate_data(self, X, y)
        base = clone(self.estimator if self.estimator is not None else LogisticRegression(max_iter=1000))
        cv = check_cv(self.cv, y, classifier=True)
        proba = cross_val_predict(base, X, y, cv=cv, method="predict_proba")
        self.estimator_ = base.fit(X, y)
        self.classes_ = self.estimator_.classes_
        confidence = proba.max(axis=1)
        correct = self.classes_[proba.argmax(axis=1)] == y
        if self.method == "auto":
            self.method_, self.method_scores_ = choose_method(confidence, correct, self.cost_wrong, self.cost_ask)
        else:
            self.method_, self.method_scores_ = self.method, {}
        self.calibrator_ = TopLabelCalibrator(method=self.method_).fit(confidence, correct)
        self.threshold_ = threshold(self.cost_wrong, self.cost_ask)
        return self

    def predict_proba(self, X):
        check_is_fitted(self)
        X = validate_data(self, X, reset=False)
        return self.estimator_.predict_proba(X)

    def predict(self, X):
        proba = self.predict_proba(X)  # raises NotFittedError first
        return self.classes_[proba.argmax(axis=1)]

    def decide(self, X):
        """(label, calibrated confidence, ask) per row: ask is True where
        the confidence is below the threshold the costs give."""
        proba = self.predict_proba(X)
        label = self.classes_[proba.argmax(axis=1)]
        confidence = self.calibrator_.predict(proba.max(axis=1))
        return label, confidence, confidence < self.threshold_

    def gate_report(self, X, y):
        """What the gate does on labelled data: coverage, accuracy of the
        actions taken, questions asked, and mean cost per decision."""
        label, confidence, ask = self.decide(X)
        right = label == np.asarray(y)
        act = ~ask
        return {
            "coverage": float(act.mean()),
            "accuracy_of_actions": float(right[act].mean()) if act.any() else float("nan"),
            "asked": int(ask.sum()),
            "cost_per_decision": gate_cost(confidence, right, self.cost_wrong, self.cost_ask),
            "threshold": self.threshold_,
            "method": self.method_,
        }
