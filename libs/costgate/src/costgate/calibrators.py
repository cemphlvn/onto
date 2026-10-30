"""Top-label calibration: map a classifier's confidence in its chosen
class to a probability that comes true as often as it says.

All maps take confidences in [0, 1] and outcomes (was the chosen class
right?) and are monotone, so they never change which class is chosen,
only how sure the system is of it.
"""

import numpy as np
from sklearn.base import BaseEstimator
from sklearn.isotonic import IsotonicRegression
from sklearn.utils.validation import check_is_fitted

EPS = 1e-6
METHODS = ("identity", "platt", "isotonic", "venn_abers")  # simplest first


def _logit(p):
    p = np.clip(np.asarray(p, dtype=float), EPS, 1 - EPS)
    return np.log(p / (1 - p))


def _sigmoid(z):
    return 1.0 / (1.0 + np.exp(-z))


def _platt(conf, correct, pull):
    """Logistic regression of the outcome on logit(conf), with a Gaussian
    prior of weight `pull` centred on the identity (a = 1, b = 0); damped
    Newton, as onto's calibrate::Platt."""
    x, y = _logit(conf), np.asarray(correct, dtype=float)
    a, b = 1.0, 0.0

    def objective(a, b):
        z = a * x + b
        return np.sum(np.where(y > 0, np.logaddexp(0, -z), np.logaddexp(0, z))) + 0.5 * pull * ((a - 1) ** 2 + b**2)

    for _ in range(100):
        q = _sigmoid(a * x + b)
        r, s = q - y, q * (1 - q)
        ga, gb = np.sum(r * x) + pull * (a - 1), np.sum(r) + pull * b
        haa, hab, hbb = np.sum(s * x * x) + pull, np.sum(s * x), np.sum(s) + pull
        det = haa * hbb - hab * hab
        if abs(det) < 1e-12:
            break
        da, db = (hbb * ga - hab * gb) / det, (haa * gb - hab * ga) / det
        t, before = 1.0, objective(a, b)
        while objective(a - t * da, b - t * db) > before and t > 1e-6:
            t /= 2
        a, b = a - t * da, b - t * db
        if t * (abs(da) + abs(db)) < 1e-10:
            break
    return a, b


class TopLabelCalibrator(BaseEstimator):
    """Calibrates confidences against outcomes.

    Parameters
    ----------
    method : {"identity", "platt", "isotonic", "venn_abers"}
        "platt" is two parameters and suits tens to hundreds of labels;
        "isotonic" is free-form and wants about a thousand; "venn_abers"
        averages two isotonic fits and is well behaved on few labels but
        slow to predict.
    pull : float
        Platt only: weight of the prior towards the identity, in labels.
    """

    def __init__(self, method="platt", pull=1.0):
        self.method = method
        self.pull = pull

    def fit(self, confidence, correct):
        c = np.asarray(confidence, dtype=float).ravel()
        y = np.asarray(correct, dtype=bool).ravel()
        if c.shape != y.shape:
            raise ValueError("confidence and correct must have the same length")
        if self.method not in METHODS:
            raise ValueError(f"method must be one of {METHODS}, got {self.method!r}")
        if self.method == "platt":
            self.a_, self.b_ = _platt(c, y, self.pull)
        elif self.method == "isotonic":
            self.iso_ = IsotonicRegression(out_of_bounds="clip", y_min=0, y_max=1).fit(c, y.astype(float))
        elif self.method == "venn_abers":
            self.c_, self.y_ = c, y.astype(float)
        self.n_ = len(c)
        return self

    def predict(self, confidence):
        check_is_fitted(self, "n_")
        c = np.asarray(confidence, dtype=float).ravel()
        if self.method == "identity":
            return np.clip(c, 0.0, 1.0)
        if self.method == "platt":
            return _sigmoid(self.a_ * _logit(c) + self.b_)
        if self.method == "isotonic":
            return self.iso_.predict(c)
        out = np.empty(len(c))
        for i, v in enumerate(c):
            ends = []
            for label in (0.0, 1.0):
                iso = IsotonicRegression(out_of_bounds="clip", y_min=0, y_max=1)
                iso.fit(np.append(self.c_, v), np.append(self.y_, label))
                ends.append(float(iso.predict([v])[0]))
            out[i] = (ends[0] + ends[1]) / 2
        return out


def threshold(cost_wrong, cost_ask):
    """Act on a prediction only when its calibrated probability of being
    right is at least 1 - cost_ask / cost_wrong; below, asking costs less
    in expectation. A wrong action worth ten questions gives 0.9."""
    if cost_wrong <= 0:
        return 0.0
    return float(np.clip(1.0 - cost_ask / cost_wrong, 0.0, 1.0))


def gate_cost(prob, correct, cost_wrong, cost_ask):
    """Mean cost per decision when the gate acts at prob >= threshold:
    cost_wrong for each wrong action, cost_ask for each question."""
    t = threshold(cost_wrong, cost_ask)
    prob, correct = np.asarray(prob, dtype=float), np.asarray(correct, dtype=bool)
    act = prob >= t
    return float(np.mean(np.where(act, np.where(correct, 0.0, cost_wrong), cost_ask)))
