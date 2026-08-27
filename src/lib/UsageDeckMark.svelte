<script lang="ts">
  import usagedeckMark from '../assets/usagedeck-mark.png';

  interface Props {
    size?: number;
    decorative?: boolean;
  }

  let { size = 18, decorative = true }: Props = $props();
</script>

<img
  src={usagedeckMark}
  width={size}
  height={size}
  alt={decorative ? '' : 'UsageDeck'}
  aria-hidden={decorative}
  draggable="false"
/>

<style>
  img {
    display: block;
    object-fit: contain;
  }
</style>
