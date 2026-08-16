import { Group, Text } from '@mantine/core';

export function StatusDot({ online }: { online: boolean }) {
  return (
    <Group gap="xs" wrap="nowrap">
      <span
        style={{
          display: 'inline-block',
          width: 10,
          height: 10,
          borderRadius: '50%',
          background: online ? 'var(--mantine-color-green-6)' : 'var(--mantine-color-gray-5)',
        }}
      />
      <Text size="sm">{online ? 'online' : 'offline'}</Text>
    </Group>
  );
}
