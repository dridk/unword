import doc2text
from pathlib import Path

FIXTURE = Path(__file__).parent / "fixtures" / "1000.doc"


def test_parse_doc_returns_document():
    data = FIXTURE.read_bytes()
    doc = doc2text.parse_doc(data)
    assert isinstance(doc.body_text, str)
    assert isinstance(doc.textboxes, list)


def test_body_text_not_empty():
    data = FIXTURE.read_bytes()
    doc = doc2text.parse_doc(data)
    assert len(doc.body_text) > 0


def test_invalid_data_raises():
    import pytest
    with pytest.raises(ValueError):
        doc2text.parse_doc(b"not a doc file")


def test_repr():
    data = FIXTURE.read_bytes()
    doc = doc2text.parse_doc(data)
    r = repr(doc)
    assert r.startswith("Document(")
