# Pyramid planning regression inputs

`upstream-15.json` and `upstream-44.json` are unchanged copies of the licensed
synthetic complete decks from basic-route-planning's accepted solver. Their
expected minimum unit costs are 15 and 44. The retained attribution and MIT
notice are in `../../../third_party/README.md` and
`../../../third_party/pyramid-solver-LICENSE`.

The v6 and v7 JSON documents are unchanged copies of the matching accepted
acquisition exports from basic-route-planning's `Evidence/` directory. The v6
48-action CSV is also copied unchanged. The v7 49-action CSV transcribes only
the ordered actions and locations from the user-supplied successful sequence in
basic-route-planning `docs/Acceptance-0.8.4.md`. It preserves pair selection
order and is a regression oracle, not a new capture or guest-input observation.
The matching typed actions are independently replayed by the xSAR tests.

The v7 oracle contains 24 MATCH, 20 MOVE, 4 KING and one RECYCLE. Its final
position has no remaining tableau or pile cards. The v6 and v7 deals are
separate fixtures and must never share a route. These retained inputs do not
authenticate a current live board, establish automated QMP acceptance or grant
input authority. JSON-dependent fixture tests require both `pyramid_search`
and `pyramid_json`; pure rules and coordinate tests require no JSON feature.
