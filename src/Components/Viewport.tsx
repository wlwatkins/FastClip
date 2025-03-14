import { Flex } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import Clip from "./Clip";
import { useEffect, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import New from "./NewClip";
import Settings from "./SettingsClip";
import { IconArrowBigDown } from "@tabler/icons-react";
import { Text } from '@mantine/core';

export default function ListOfClips() {
    const { height } = useViewportSize();

    const [clips, SetClips] = useState<Array<FastClip>>([])

    useEffect(() => {
        const unlistenPromises: Promise<UnlistenFn>[] = [];

        unlistenPromises.push(listen('update_clips', (event) => {
            console.log(event.payload as string);
            SetClips(event.payload as Array<FastClip>);
        }));

        return () => {
            unlistenPromises.forEach((unlistenPromise) => {
                unlistenPromise.then((unlisten) => unlisten());
            });
        };
    }, []);


    useEffect(() => {
        const initialize = async () => {
            invoke('get_clips')
                .then((message) => {
                    console.log(message);
                    SetClips(message as Array<FastClip>);
                })
                .catch((error) => {
                    console.error(error);
                });

        };

        initialize(); // Call the async function inside useEffect
    }, []); // Empty dependency array ensures it runs only on mount




    return (

        <>

            {clips.length === 0 ? (
                <Flex
                    w="95%"
                    maw="500px"
                    gap="md"
                    align="center"
                    justify="center"
                    direction="column"
                    style={{ height: `${height}px` }}
                    p={10}
                >


                    <Text c="dimmed">Create your first clip +</Text>
                    <IconArrowBigDown
                        size={60}
                        stroke={1}
                        color="var(--mantine-color-gray-filled)"
                    />
                </Flex>
            ) : (
                <Flex
                    align="center"
                    direction="column"
                    style={{ height: `${height}px` }}
                    pt={50}
                >

                     {/* <ScrollArea
                        h={height - 80}
                        scrollbars="y"
                        >  */}
                        <Flex
                            gap={10}
                            direction="column"
                            w="100%"
                        >
                            {clips.map((clip) => (
                                <Clip key={clip.id} fast_clip={clip} /> // Assuming `clip.id` is unique
                            ))}
                        </Flex>
                     {/* </ScrollArea>  */}
                 </Flex>
            )}
            <New />
            <Settings />
        </>

    )
}
