import base64


def armor(label: str, der_bytes: bytes) -> bytes:
    body = base64.b64encode(der_bytes).decode("ascii")
    lines = [body[i:i + 64] for i in range(0, len(body), 64)]
    text = (
        f"-----BEGIN {label}-----\n"
        + "\n".join(lines)
        + "\n"
        + f"-----END {label}-----\n"
    )
    return text.encode("ascii")
