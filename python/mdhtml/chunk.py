from itertools import batched

from fast5ever import Element, Text, parse_fragment

from ._native import (md2mdhtml as _md2mdhtml, md_chunks as _md_chunks,
    md_chunks_greedy as _md_chunks_greedy, md_chunks_structural as _md_chunks_structural,
    md_chunks_structural_batch as _md_chunks_structural_batch)


START_PENALTIES = {"h1": 0, "h2": 0, "h3": 1, "h4": 2, "h5+": 4, "body": 4}


def md_chunks(markdown: str, target_words: int = 700) -> list[dict]:
    "Split Markdown hierarchically, returning each chunk's Markdown and original start boundary."
    return [dict(md=md, start=start) for md,start in _md_chunks(markdown, target_words)]


def md_chunks_greedy(markdown: str, target_words: int = 700, length_scale: int = 200) -> list[dict]:
    "Split Markdown at parsed block boundaries using a local length-and-boundary score."
    return [dict(md=md, start=start) for md,start in _md_chunks_greedy(markdown, target_words, length_scale)]


def md_chunks_structural(markdown: str, target_words: int = 700) -> list[dict]:
    "Split Markdown hierarchically, cutting only at parsed top-level block boundaries."
    return [dict(md=md, start=start) for md,start in _md_chunks_structural(markdown, target_words)]


def md_chunks_structural_batch(markdowns, target_words: int = 700, batch_size: int = 128) -> list[list[dict]]:
    "Chunk a batch of Markdown documents during one GIL release."
    return [[dict(md=md, start=start) for md,start in chunks] for batch in batched(markdowns, batch_size)
        for chunks in _md_chunks_structural_batch(batch, target_words)]


def _visible_words(markdown):
    html,_,_ = _md2mdhtml(markdown)
    def text(node):
        if isinstance(node, Text): return node.text
        if isinstance(node, Element) and node.name in ("script", "template"): return ""
        return " ".join(text(child) for child in node.children)
    return len(text(parse_fragment(html)).split())


def score_chunks(chunks, target_words=700, length_scale=50, start_penalties=None):
    "Score chunks by mean start-boundary penalty plus mean absolute target-length error."
    chunks = list(chunks)
    if not chunks: raise ValueError("chunks must not be empty")
    if target_words <= 0 or length_scale <= 0: raise ValueError("target_words and length_scale must be positive")
    penalties = START_PENALTIES if start_penalties is None else start_penalties
    starts,lengths,start_costs = [],[],[]
    for chunk in chunks:
        start = chunk["start"]
        key = "h5+" if start.startswith("h") and int(start[1:]) >= 5 else start
        if key not in penalties: raise ValueError(f"no penalty for chunk start {start!r}")
        starts.append(start)
        lengths.append(_visible_words(chunk["md"]))
        start_costs.append(penalties[key])
    start_score = sum(start_costs) / len(chunks)
    length_score = sum(abs(length-target_words) for length in lengths) / len(chunks) / length_scale
    return dict(score=start_score+length_score, start_score=start_score, length_score=length_score,
        lengths=lengths, starts=starts, max_length=max(lengths))
