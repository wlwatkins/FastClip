<script lang="ts">
  import TitleBar from "./lib/components/TitleBar.svelte";
</script>

<div class="flex h-screen w-screen min-w-[250px] flex-col overflow-hidden bg-zinc-900 text-zinc-100">
  <TitleBar />
  <!--
    The clip list is WP-05. This scaffold (WP-04) has no features: no clip
    list, no CRUD, no search — see docs/src/work/wp-04-frontend-scaffold.md.
  -->
  <main class="flex-1 overflow-y-auto" aria-label="Clips"></main>
</div>
