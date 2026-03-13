<script setup lang="ts">
import { ref, useAttrs } from "vue";
import PrimeSlider from "primevue/slider";
import type { SliderSlideEndEvent } from "primevue/slider";

const props = withDefaults(
  defineProps<{
    modelValue: number | number[];
    min?: number;
    max?: number;
    step?: number | null;
    orientation?: "horizontal" | "vertical";
    range?: boolean;
    disabled?: boolean;
    tabindex?: number;
    ariaLabelledby?: string | null;
    ariaLabel?: string | null;
  }>(),
  {
    min: 0,
    max: 100,
    step: null,
    orientation: "horizontal",
    range: false,
    disabled: false,
    tabindex: 0,
    ariaLabelledby: null,
    ariaLabel: null
  }
);

const emit = defineEmits<{
  "update:modelValue": [value: number | number[]];
  slideend: [event: SliderSlideEndEvent];
}>();

const attrs = useAttrs();
const sliderRef = ref<{ $el?: Element | null } | null>(null);
const isTrackDragging = ref(false);
const lastTrackValue = ref<number | number[] | null>(null);
let activePointerId: number | null = null;

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function normalizeValue(rawValue: number, currentValue: number) {
  if (!props.step) {
    return clamp(rawValue, props.min, props.max);
  }

  const diff = rawValue - currentValue;

  if (diff < 0) {
    return clamp(currentValue + Math.ceil(rawValue / props.step - currentValue / props.step) * props.step, props.min, props.max);
  }

  if (diff > 0) {
    return clamp(currentValue + Math.floor(rawValue / props.step - currentValue / props.step) * props.step, props.min, props.max);
  }

  return clamp(currentValue, props.min, props.max);
}

function getSliderElement() {
  const element = sliderRef.value?.$el;

  return element instanceof HTMLElement ? element : undefined;
}

function getPointerValue(event: PointerEvent) {
  const sliderElement = getSliderElement();

  if (!sliderElement) {
    return props.modelValue;
  }

  const rect = sliderElement.getBoundingClientRect();
  const ratio = props.orientation === "vertical"
    ? clamp((rect.bottom - event.clientY) / rect.height, 0, 1)
    : clamp((event.clientX - rect.left) / rect.width, 0, 1);
  const rawValue = props.min + ratio * (props.max - props.min);

  if (Array.isArray(props.modelValue)) {
    const values = [...props.modelValue];
    const distances = values.map((value) => Math.abs(value - rawValue));
    const handleIndex = distances[0] <= distances[1] ? 0 : 1;
    values[handleIndex] = normalizeValue(rawValue, values[handleIndex]);

    return values;
  }

  return normalizeValue(rawValue, props.modelValue);
}

function onPointerDown(event: PointerEvent) {
  if (props.disabled || event.button !== 0) {
    return;
  }

  const target = event.target as HTMLElement | null;

  if (target?.closest(".p-slider-handle")) {
    return;
  }

  const sliderElement = getSliderElement();

  if (!sliderElement) {
    return;
  }

  isTrackDragging.value = true;
  activePointerId = event.pointerId;
  sliderElement.setPointerCapture(event.pointerId);
  lastTrackValue.value = getPointerValue(event);
  emit("update:modelValue", lastTrackValue.value);
  event.preventDefault();
}

function onPointerMove(event: PointerEvent) {
  if (!isTrackDragging.value || activePointerId !== event.pointerId) {
    return;
  }

  lastTrackValue.value = getPointerValue(event);
  emit("update:modelValue", lastTrackValue.value);
}

function finishTrackDrag(event: PointerEvent) {
  const sliderElement = getSliderElement();

  if (sliderElement && activePointerId !== null && sliderElement.hasPointerCapture(activePointerId)) {
    sliderElement.releasePointerCapture(activePointerId);
  }

  const slideEndValue = lastTrackValue.value ?? props.modelValue;

  isTrackDragging.value = false;
  activePointerId = null;
  lastTrackValue.value = null;
  emit("slideend", { originalEvent: event, value: slideEndValue });
}

function onPointerUp(event: PointerEvent) {
  if (!isTrackDragging.value || activePointerId !== event.pointerId) {
    return;
  }

  finishTrackDrag(event);
}

function onPointerCancel(event: PointerEvent) {
  if (!isTrackDragging.value || activePointerId !== event.pointerId) {
    return;
  }

  finishTrackDrag(event);
}
</script>

<template>
  <div
    class="app-slider"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerCancel"
  >
    <PrimeSlider
      ref="sliderRef"
      :model-value="modelValue"
      :min="min"
      :max="max"
      :step="step ?? undefined"
      :orientation="orientation"
      :range="range"
      :disabled="disabled"
      :tabindex="tabindex"
      :aria-labelledby="ariaLabelledby"
      :aria-label="ariaLabel"
      v-bind="attrs"
      @update:model-value="(value) => emit('update:modelValue', value)"
      @slideend="(event) => emit('slideend', event)"
    />
  </div>
</template>

<style scoped>
.app-slider {
  touch-action: none;
}

.app-slider :deep(.p-slider-horizontal) {
  position: relative;
  height: 1.75rem;
}

.app-slider :deep(.p-slider-horizontal .p-slider-range),
.app-slider :deep(.p-slider-horizontal::before) {
  position: absolute;
  top: 50%;
  height: 0.5rem;
  border-radius: 999px;
  transform: translateY(-50%);
}

.app-slider :deep(.p-slider-horizontal::before) {
  content: "";
  inset-inline: 0;
  background: color-mix(in srgb, var(--accent) 18%, transparent);
}

.app-slider :deep(.p-slider-horizontal .p-slider-range) {
  background: var(--accent);
}

.app-slider :deep(.p-slider-horizontal .p-slider-handle) {
  width: 1.2rem;
  height: 1.2rem;
  margin-top: -0.6rem;
}
</style>

