def _der_encode_length(length: int) -> bytes:
    if length < 0x80:
        return bytes([length])

    chunks = []
    while length:
        chunks.append(length & 0xFF)
        length >>= 8
    chunks.reverse()
    return bytes([0x80 | len(chunks), *chunks])


class _ASN1String(str):
    tag = None

    def dump(self) -> bytes:
        value = self.encode("utf-8")
        return bytes([self.tag]) + _der_encode_length(len(value)) + value


class VisibleString(_ASN1String):
    tag = 0x1A


class UTF8String(_ASN1String):
    tag = 0x0C


class Sequence:
    _fields = []

    def __init__(self, values):
        self._values = values

    def dump(self) -> bytes:
        encoded = bytearray()
        for name, field_type in self._fields:
            value = self._values[name]
            if not isinstance(value, field_type):
                value = field_type(value)
            encoded.extend(value.dump())
        return bytes([0x30]) + _der_encode_length(len(encoded)) + bytes(encoded)
