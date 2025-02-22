import { ActionIcon, Box, Button, ColorPicker, Modal, NumberInput, Popover, Switch, Textarea, TextInput } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import { useDisclosure } from "@mantine/hooks";
import { IconClipboardCheckFilled, IconPlus, IconSend, IconTag } from '@tabler/icons-react';
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";


export default function New() {
    const [opened, { open, close }] = useDisclosure(false);
    const [colour, onChangeColour] = useState('#000814');
    const [clear_time_enable, setClearTimeEnable] = useState(false);

    const form = useForm({
        mode: 'uncontrolled',
        initialValues: {
            value: '',
            label: '',
            clear_time: 60
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
            clear_time: (value) => {
                if (value === undefined) return null; // Allow undefined
                if (typeof value !== 'number' || value < 1) {
                    return 'Must be a positive number';
                }
                return null; // No error
            }
        },
    });

    const handleSubmit = (values: typeof form.values) => {
        const clip = new FastClip(
            values.value, 
            values.label, 
            "📌", 
            colour, 
            true,
            clear_time_enable ? values.clear_time : undefined
        );
        console.log(`Form submitted with: ${clip}`);


        invoke('new_clip', { "clip": clip })
            .catch((error) => console.error(error));
        close();
        form.reset()
    };

    return (

        <>
            <Modal
                opened={opened}
                onClose={close}
                title="Add a new FastClip"
                fullScreen
                overlayProps={{
                    backgroundOpacity: 0.55,
                    blur: 3,
                }}
                transitionProps={{ transition: 'pop-bottom-right', duration: 150 }}
            >

                <form onSubmit={form.onSubmit(handleSubmit)} >

                    <TextInput
                        leftSection={<IconTag size={16} />}
                        withAsterisk
                        mt={10}
                        label="Label"
                        placeholder="This will be displayed"
                        key={form.key('label')}
                        {...form.getInputProps('label')}
                    />


                    <Textarea
                        mt={10}
                        leftSection={<IconClipboardCheckFilled size={16} />}
                        withAsterisk
                        label="What do you want to paste?"
                        resize="vertical"
                        placeholder="This will be pastable"
                        key={form.key('value')}
                        {...form.getInputProps('value')}
                    />

                    <Switch
                        mt={10}
                        mb={5}
                        size="xs"
                        checked={clear_time_enable}
                        labelPosition="left"
                        label="Clear clipboard after N secs"
                        onChange={(e) => setClearTimeEnable(e.target.checked)} // Extract boolean value
                     />
                    <NumberInput
                        disabled={!clear_time_enable}
                        min={0}
                        step={1}
                        value={form.values.clear_time} 
                        key={form.key('clear_time')}
                        {...form.getInputProps('clear_time')}
                    />


                    <Popover trapFocus position="bottom" withArrow shadow="md" >
                        <Popover.Target>
                            <Button autoContrast mt={10} fullWidth radius="xl" color={colour}>Select colour</Button>
                        </Popover.Target>
                        <Popover.Dropdown>
                               <ColorPicker
                                                mt={10} 
                                                swatchesPerRow={12}
                                                format="hex"
                                                onChange={onChangeColour}
                                                defaultValue={colour}
                                                withPicker={false}
                                                swatches={['#000814', '#001D3D', '#003566', '#FFC300', '#FFD60A']}
                                            />
                        </Popover.Dropdown>
                    </Popover>




                    <Box className='absolute  bottom-0 right-0'>
                        <ActionIcon variant="filled" color="blue" aria-label="Add" size="xl" m={10} type="submit">
                            <IconSend style={{ width: '70%', height: '70%' }} stroke={1.5} />
                        </ActionIcon>
                    </Box>


                </form>
            </Modal>


            <Box className='absolute  bottom-0 right-0'>
                <ActionIcon variant="filled" color="green" aria-label="Add" radius="xl" size="xl" m={10} onClick={open}>
                    <IconPlus style={{ width: '70%', height: '70%' }} stroke={1.5} />
                </ActionIcon>
            </Box>

        </>
    )
}
