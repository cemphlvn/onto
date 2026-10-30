"""costgate: calibrate a classifier's confidence and decide by cost."""

from .calibrators import METHODS, TopLabelCalibrator, gate_cost, threshold
from .gate import CostGatedClassifier, choose_method

__all__ = ["METHODS", "CostGatedClassifier", "TopLabelCalibrator", "choose_method", "gate_cost", "threshold"]
__version__ = "0.1.0"
