import pytest

from mdhtml import md_chunks, score_chunks


def test_md_chunks_reports_real_boundaries_and_breadcrumbs():
    text = "# Title\n\n" + "one two three four five six.\n\n## Section\n\n" + "seven eight nine ten eleven twelve.\n\n### Detail\n\n" + "thirteen fourteen fifteen sixteen."
    chunks = md_chunks(text, 8)
    assert chunks[0]["start"] == "h1"
    assert any(chunk["start"] == "h2" for chunk in chunks)
    assert all(chunk["md"].startswith("#") for chunk in chunks)


def test_chunk_score_uses_visible_words_and_start_penalties():
    chunks = [dict(md="# T\n\none **two** <template data-op=\"x:y\">hidden words</template>", start="h1"),
        dict(md="# T\n\n### S\n\nthree four", start="h3")]
    score = score_chunks(chunks, target_words=4, length_scale=2)
    assert score["lengths"] == [3, 4]
    assert score["starts"] == ["h1", "h3"]
    assert score["start_score"] == 0.5 and score["length_score"] == 0.25 and score["score"] == 0.75


def test_chunk_arguments_are_validated():
    with pytest.raises(ValueError, match="positive"): md_chunks("text", 0)
    with pytest.raises(ValueError, match="must not be empty"): score_chunks([])
