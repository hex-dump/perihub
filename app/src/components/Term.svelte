<script lang="ts">
  // A word with its explanation on hover and for screen readers (lib/glossary.ts).
  import type { Snippet } from "svelte";
  import { GLOSSARY, type GlossaryTerm } from "../lib/glossary";

  interface Props {
    term: GlossaryTerm;
    children: Snippet;
  }

  let { term, children }: Props = $props();
</script>

<abbr class="term" title={GLOSSARY[term]}>{@render children()}</abbr>
