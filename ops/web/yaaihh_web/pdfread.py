"""pymupdf4llm on one PDF, in a process of its own: `pdfread.py PDF [DIR...]` prints [[page, markdown], ...] as JSON.
DIRs (fetch.reader_path) come first on its path, for a pymupdf4llm installed outside the server's Python. Exit 3: no
pymupdf4llm."""
import json, sys

sys.path[:0] = sys.argv[2:]
try:
    import pymupdf4llm
except ImportError:
    sys.exit(3)
if hasattr(pymupdf4llm, "use_layout"):
    pymupdf4llm.use_layout(False)  # the layout model (onnxruntime) costs 20 times the CPU and is not installed
chunks = pymupdf4llm.to_markdown(sys.argv[1], page_chunks=True)
json.dump([[c["metadata"].get("page", i + 1), c["text"]] for i, c in enumerate(chunks)], sys.stdout)
