import sys
from docling.document_converter import DocumentConverter

def ingest(path):
    converter = DocumentConverter()
    result = converter.convert(path)
    print(result.document.export_to_markdown())

if __name__ == "__main__":
    ingest(sys.argv[1])
