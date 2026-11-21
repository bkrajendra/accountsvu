import re

# Valid Indian State/UT codes (01-38, 97 for Other Territory, 99 for Centre Jurisdiction)
VALID_STATE_CODES = {
    "01", "02", "03", "04", "05", "06", "07", "08", "09", "10",
    "11", "12", "13", "14", "15", "16", "17", "18", "19", "20",
    "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    "31", "32", "33", "34", "35", "36", "37", "38", "97", "99"
}

# Base-36 character set used for GSTIN checksum calculation
BASE36_CHARS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"

# Standard 15-character GSTIN pattern:
# 2-digit state code + 10-char PAN + 1-char entity code + 'Z' + 1-char checksum
GSTIN_REGEX = re.compile(r"^[0-9]{2}[A-Z]{5}[0-9]{4}[A-Z]{1}[1-9A-Z]{1}Z[0-9A-Z]{1}$")


def calculate_gstin_checksum(first_14: str) -> str:
    """Calculate the expected 15th checksum character using Luhn Mod-36."""
    total = 0
    for index, char in enumerate(first_14):
        code_point = BASE36_CHARS.index(char)
        # Alternate multipliers: 1 for even indices (0, 2, ...), 2 for odd indices (1, 3, ...)
        factor = 2 if index % 2 else 1
        product = code_point * factor
        # Add quotient and remainder when divided by 36
        total += (product // 36) + (product % 36)

    checksum_value = (36 - (total % 36)) % 36
    return BASE36_CHARS[checksum_value]


def validate_gstin(gstin: str) -> tuple[bool, str]:
    """
    Validate a GSTIN offline.
    Returns (is_valid, reason_message).
    """
    if not isinstance(gstin, str):
        return False, "Input must be a string."

    gstin = gstin.strip().upper()

    if len(gstin) != 15:
        return False, f"Invalid length ({len(gstin)} characters; expected 15)."

    if not GSTIN_REGEX.match(gstin):
        return False, "Format mismatch (invalid PAN structure, entity code, or missing 'Z')."

    state_code = gstin[:2]
    if state_code not in VALID_STATE_CODES:
        return False, f"Invalid state/UT code: '{state_code}'."

    expected_checksum = calculate_gstin_checksum(gstin[:14])
    if gstin[14] != expected_checksum:
        return False, f"Checksum mismatch (expected '{expected_checksum}', got '{gstin[14]}')."

    return True, "Valid GSTIN format and checksum."


# --- Example Usage ---
if __name__ == "__main__":
    test_gstins = [
        "24AALCA9876P1ZO",  # Valid format & checksum (Maharashtra)
        "33AAJFB4321M2ZJ",  # Valid format & checksum (Tamil Nadu)
        "07AFZPK7190K1ZI",  # Typo in PAN -> fails checksum
        "27AABCT1234C1ZW",  # Invalid state code
        "29AAKCS5678D1ZU",  # Missing default 'Z' at 14th position
    ]

    for code in test_gstins:
        is_valid, message = validate_gstin(code)
        print(f"{code} -> {is_valid} ({message})")
