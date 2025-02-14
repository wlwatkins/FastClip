import { ActionIcon, Box, Button, Center, Flex, Grid } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import { IconPhoto, IconSettings, IconHeart, IconEdit, IconTrashX, IconEyeOff, IconAdjustments, IconPlus } from '@tabler/icons-react';
import Item from "./Clip";
import { useEffect } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";






export default function ListOfClips() {

    const [state, setState] = useSetState({
        value: 'John',
        icon: undefined,
      });


    const buttonLabels = [state, 'Button 2', 'Button 3'];

    const { height } = useViewportSize();


    useEffect(() => {
        const unlistenPromises: Promise<UnlistenFn>[] = [];

        unlistenPromises.push(listen('basler_update_get', (event) => setConnected(event.payload as boolean)));
   
        return () => {
            // Unlisten to all events
            unlistenPromises.forEach((unlistenPromise) => {
                unlistenPromise.then((unlisten) => unlisten());
            });
        };
    }, []);





    return (

        <Flex
            w="100%"

            maw="500px"
            gap="md"
            align="center"
            direction="column"
            style={{ height: `${height}px` }}
            p={10}
        >
            {buttonLabels.map((label, index) => (
                <Item key={index} label={label} />
            ))}


        </Flex>

    )
}
