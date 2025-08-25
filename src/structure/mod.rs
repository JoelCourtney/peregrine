/*
* List of node structures:
* - Stack
*   - evaluates to last element
*   - elements can only access previous element
*   - nodes only inserted at end
* - Accumulator
    - Uses the Accumulate trait
    - Only accepts Accumulate::Diff nodes
    - evaluates to the accumulated value
* - Sequence
    - evaluates to ordered list of element results
    - accepts nodes the eval to a given type
    - can be queried internally and externally at any index
* - Stage
    - evaluates to last element
    - elements can only access previous element
    - nodes inserted at any unique index
* - ParStage
    - evaluates to last element
    - elements can only access previous element
    - nodes inserted at any index
    - nodes produce Accumulate::Diff
*/
