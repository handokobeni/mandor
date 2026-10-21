<script lang="ts">
  import LarkAppForm from './LarkAppForm.svelte';
  import LarkLoginStatus from './LarkLoginStatus.svelte';
</script>

<section>
  <h1>Lark</h1>
  <p class="lede">
    Connect the Lark Custom App and the Base that holds your Sprint Tasks. Lark stays the source of truth for titles,
    descriptions, AC, and assignees.
  </p>
</section>

<section><LarkLoginStatus /></section>

<section><LarkAppForm /></section>
