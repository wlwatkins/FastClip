# WP-11 — Copy deck and README

**Objective:** every string the user reads is written once, deliberately, and
the README stops making a claim the software cannot support.

**Depends on:** WP-07, WP-09.

**Inputs:** [copy deck](../product/copy.md),
[ADR-0002](../architecture/adr/0002-threat-model.md).

## Work

### frontend-dev

Fill in the copy deck: validation messages, confirmations, and every failure
message including the crypto and import variants. Then replace inline strings
in the components with the deck's wording.

Every error variant in the contract maps to a human sentence. A raw error must
never reach the user.

### backend-dev

Rewrite the README. The current text says **"DO NOT USE FOR PASSWORDS"**. It is
reworded, not deleted: *clips are encrypted at rest, but FastClip is not a
password manager and does not protect against software running under your
account.*

Update the feature list and remove completed to-do items.

### critic

The two security strings are claims about what the software protects.
Overstating either is `BLOCK`. Check them against ADR-0002 word for word, and
check that "your clips are secure" appears nowhere.

## Definition of done

- No user-facing string is authored inline.
- Every contract error variant has a message.
- The README security claim matches ADR-0002.

## Risks

The temptation at this point is to delete the password warning outright,
because the encryption work is finished and it feels earned. It is not. The
threat model did not change.
