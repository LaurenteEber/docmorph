from pathlib import Path
import sys

objects = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    b"<< /Length 58 >>\nstream\nBT /F1 18 Tf 72 720 Td (DocMorph structural page 1) Tj ET\nendstream",
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]
body = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
offsets = [0]
for number, object_bytes in enumerate(objects, 1):
    offsets.append(len(body))
    body.extend(f"{number} 0 obj\n".encode())
    body.extend(object_bytes)
    body.extend(b"\nendobj\n")
xref = len(body)
body.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
body.extend(b"".join(f"{offset:010} 00000 n \n".encode() for offset in offsets[1:]))
body.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
Path(sys.argv[1]).write_bytes(body)
