import { ActionIcon, Box, Drawer } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconAdjustments } from '@tabler/icons-react';



export default function Settings() {
    const [opened, { open, close }] = useDisclosure(false);

    // const form = useForm({
    //     mode: 'uncontrolled',
    //     initialValues: {
    //         value: '',
    //         label: '',
    //     }, validate: {
    //         value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
    //         label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
    //     },

    // });

    // const handleSubmit = (values: typeof form.values) => {
    //     const clip = new FastClip(values.value, values.label, "📌");
    //     console.log('Form submitted with:', clip);

    //     invoke('new_clip', { clip })
    //         .then((message) => console.log(message))
    //         .catch((error) => console.error(error));
    //     close();
    //     form.reset()
    // };

    return (

        <>
            <Drawer opened={opened} onClose={close} title="Authentication">
                {/* Drawer content */}
            </Drawer>


            <Box className='absolute  bottom-0 left-0'>
                <ActionIcon variant="light" color="gray" aria-label="Add" size="lg" m={10} onClick={open}>
                    <IconAdjustments style={{ width: '70%', height: '70%' }} stroke={1.5} />
                </ActionIcon>

            </Box>

        </>
    )
}
