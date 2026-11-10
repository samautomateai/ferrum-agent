# Quality evidence agent

Check actual CI run SHA, rustfmt, clippy, tests, coverage >=90%, cargo audit/deny, gitleaks, actionlint/zizmor, source limits, and Linux AMD64/ARM64 plus Windows AMD64. Capture exact evidence and missing gates. Never equate configured coverage threshold with measured coverage; never report green based on an older commit. Fail closed on absent results.
