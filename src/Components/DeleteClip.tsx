import { Button, Flex, Modal } from "@mantine/core";
import { hasLength, useForm } from "@mantine/form";
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";


interface DeleteProps {
    fast_clip: FastClip;
    opened: boolean;
    open: () => void;
    close: () => void;
}

export default function Delete({ fast_clip, opened, close }: DeleteProps) {

    const form = useForm({
        mode: 'uncontrolled',
        initialValues: {
            value: fast_clip.value,
            label: fast_clip.label,
        }, validate: {
            value: hasLength({ min: 1 }, 'Must be at least 1 characters'),
            label: hasLength({ min: 3 }, 'Must be at least 3 characters'),
        },
    });

    const handleDelClip = () => {
        invoke('del_clip', { "clip_id": fast_clip.id })
            .then((message) => console.log(message))
            .catch((error) => console.error(error));
        close();
        form.reset()
    };

    return (
            <Modal
                opened={opened}
                onClose={close}
                title={`Are you sure you want to delete "${fast_clip.label}" ?`}
                centered
                withCloseButton={false}
                overlayProps={{
                    backgroundOpacity: 0.55,
                    blur: 3,
                }}
                transitionProps={{ transition: 'pop-bottom-right', duration: 150 }}
            >
                <Flex gap="md" justify="center" align="center" direction="row" wrap="wrap">
                    <Button variant="filled" color="red" onClick={handleDelClip}>Yes</Button>
                    <Button variant="default" onClick={close}>No</Button>
                </Flex>
            </Modal>
    )
}
