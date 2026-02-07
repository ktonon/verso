"""Static vocabularies for encoder (input tokens) and decoder (actions)."""

from __future__ import annotations


class EncoderVocab:
    """Maps input token strings to integer IDs.

    Static vocabulary covering all token types from training_data.rs token_to_string.
    """

    PAD = 0

    # All static tokens in registration order
    _STATIC_TOKENS = [
        # Operators
        "ADD", "MUL", "POW", "NEG", "INV",
        # Trig functions
        "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN",
        # Other functions
        "SIGN", "SINH", "COSH", "TANH",
        "FLOOR", "CEIL", "ROUND",
        "MIN", "MAX", "CLAMP",
        "EXP", "LN",
        # Named constants
        "E", "SQRT2", "SQRT3", "INV_SQRT2", "SQRT3_2",
        # Structural
        "FRAC", "FRAC_PI", "IDX_LO", "IDX_HI",
    ]

    _NUM_VARS = 8       # V0..V7
    _INT_LO = -10       # I_{-10}
    _INT_HI = 20        # I_{20}
    _NUM_INDICES = 4     # IX0..IX3

    def __init__(self):
        self._token_to_id: dict[str, int] = {"<PAD>": 0}
        self._id_to_token: list[str] = ["<PAD>"]
        self._build()

    def _build(self):
        for tok in self._STATIC_TOKENS:
            self._register(tok)
        for i in range(self._NUM_VARS):
            self._register(f"V{i}")
        for i in range(self._INT_LO, self._INT_HI + 1):
            self._register(f"I_{i}")
        for i in range(self._NUM_INDICES):
            self._register(f"IX{i}")

    def _register(self, token: str):
        idx = len(self._id_to_token)
        self._token_to_id[token] = idx
        self._id_to_token.append(token)

    def encode(self, token: str) -> int:
        """Convert a token string to its integer ID. Returns PAD for unknown."""
        return self._token_to_id.get(token, self.PAD)

    def decode(self, token_id: int) -> str:
        if 0 <= token_id < len(self._id_to_token):
            return self._id_to_token[token_id]
        return "<PAD>"

    def size(self) -> int:
        return len(self._id_to_token)


class DecoderVocab:
    """Maps decoder tokens (BOS, STOP, RULE_n, POS_n) to integer IDs.

    Layout: [PAD=0, BOS=1, STOP=2, RULE_0..RULE_{n-1}, POS_0..POS_{m-1}]
    """

    PAD = 0
    BOS = 1
    STOP = 2

    def __init__(self, num_rules: int = 210, max_positions: int = 64):
        self._num_rules = num_rules
        self._max_positions = max_positions

    @property
    def rule_offset(self) -> int:
        return 3  # after PAD, BOS, STOP

    @property
    def pos_offset(self) -> int:
        return 3 + self._num_rules

    def encode_rule(self, rule_dir: int) -> int:
        return self.rule_offset + rule_dir

    def encode_pos(self, pos: int) -> int:
        return self.pos_offset + min(pos, self._max_positions - 1)

    def decode(self, token_id: int) -> tuple[str, int]:
        """Returns (type, value) where type is 'PAD', 'BOS', 'STOP', 'RULE', or 'POS'."""
        if token_id == self.PAD:
            return ("PAD", 0)
        if token_id == self.BOS:
            return ("BOS", 0)
        if token_id == self.STOP:
            return ("STOP", 0)
        if self.rule_offset <= token_id < self.pos_offset:
            return ("RULE", token_id - self.rule_offset)
        if self.pos_offset <= token_id < self.pos_offset + self._max_positions:
            return ("POS", token_id - self.pos_offset)
        return ("PAD", 0)

    def size(self) -> int:
        return 3 + self._num_rules + self._max_positions
